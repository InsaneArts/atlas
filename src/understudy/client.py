"""Real JSONL Unix-socket client. No host UI or input fallback."""
import base64
import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import socket
import time


class SessionError(RuntimeError):
    pass


def validate_helper_uid(uid):
    # Preserve the conservative UID boundary used in the live investigation.
    if type(uid) is not int or uid <= 501 or uid == os.getuid():
        raise SessionError("Target must be a distinct helper account with UID >= 502")


def require_hidden_session(status, uid):
    """Fail closed on missing identity, console state, Aqua context or geometry."""
    validate_helper_uid(uid)
    if not isinstance(status, dict) or status.get("ok") is not True:
        raise SessionError("Helper is unavailable: " + json.dumps(status))
    user = status.get("user") or {}
    session = status.get("session") or {}
    display = status.get("display") or {}
    if not all(isinstance(value, dict) for value in (user, session, display)):
        raise SessionError("Malformed session identity")
    geometry = [display.get("width"), display.get("height")]
    if (
        type(user.get("uid")) is not int
        or user.get("uid") != uid
        or session.get("onConsole") is not False
        or session.get("managerName") != "Aqua"
        or not all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in geometry)
    ):
        raise SessionError("Expected the configured helper in a hidden Aqua desktop: " + json.dumps(status))
    return status


class Client:
    def __init__(self, uid, socket_path, output_dir):
        validate_helper_uid(uid)
        self.uid = uid
        self.socket_path = str(Path(socket_path).expanduser())
        self.output_dir = Path(output_dir).expanduser()
        self.output_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        lock_dir = Path.home() / "Library/Caches/Understudy"
        lock_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        key = hashlib.sha256(self.socket_path.encode()).hexdigest()[:24]
        self.lock_path = lock_dir / (key + ".lock")

    def _rpc(self, request, label, timeout=45, persist_png=True):
        started = time.perf_counter_ns()
        timestamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
        events = []
        final = None
        try:
            payload = (json.dumps(request) + "\n").encode()
            if len(payload) > 1024 * 1024:
                raise ValueError("Request exceeds daemon's 1 MiB limit")
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(timeout)
                connection.connect(self.socket_path)
                connection.sendall(payload)
                with connection.makefile("rb") as stream:
                    for line in stream:
                        row = json.loads(line)
                        if "ok" in row:
                            final = row
                            break
                        events.append(row)
            if final is None:
                raise ConnectionError("Connection ended without a final response")
        except (OSError, ValueError) as exc:
            final = {"ok": False, "code": "transport", "error": str(exc),
                     "delivery": "unknown; observe state before retrying an action"}

        elapsed = (time.perf_counter_ns() - started) / 1e6
        frame = None
        if "png" in final:
            png = final.pop("png")
            safe_label = re.sub(r"[^a-zA-Z0-9_-]", "-", label)[:64]
            path = self.output_dir / (str(time.time_ns()) + "-" + safe_label + ".png")
            try:
                frame = base64.b64decode(png, validate=True)
                if persist_png:
                    path.write_bytes(frame)
                    final["path"] = str(path.resolve())
            except (OSError, ValueError) as exc:
                final = {"ok": False, "code": "capture-output", "error": str(exc)}
        log = {"timestamp": timestamp, "label": label, "socket": self.socket_path,
               "request": request, "elapsedMs": elapsed, "response": dict(final), "events": events}
        with (self.output_dir / "rpc.jsonl").open("a") as stream:
            stream.write(json.dumps(log) + "\n")
        final["elapsedMs"] = elapsed
        if events:
            final["events"] = events
        if frame is not None and not persist_png:
            final["_frame"] = frame
        return final

    def status(self):
        return self._rpc({"op": "hello"}, "status")

    def identity(self):
        return require_hidden_session(self.status(), self.uid)

    def capture(self, maximum=None):
        self.identity()
        request = {"op": "screenshot"}
        if maximum is not None:
            if maximum <= 0:
                raise ValueError("max-dimension must be positive")
            request["maxDimension"] = maximum
        return self._rpc(request, "capture")

    def frame(self, maximum=1600):
        """Observe without accumulating image files. Metadata remains logged."""
        self.identity()
        result = self._rpc({"op": "screenshot", "maxDimension": maximum},
                           "monitor-frame", persist_png=False)
        image = result.pop("_frame", None)
        if result.get("ok") is not True or image is None:
            raise SessionError(result.get("error", "Helper did not return a frame"))
        return result, image

    def input(self, actions):
        if not isinstance(actions, list):
            raise ValueError("Input must be a JSON array of actions")
        # Cooperative client lock only; other callers can still reach the daemon.
        with self.lock_path.open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise SessionError("Another Understudy client owns this input batch") from exc
            self.identity()
            return self._rpc({"op": "input", "actions": actions}, "input")

    def run(self, argv):
        if not argv:
            raise ValueError("Provide a command after run --")
        self.identity()
        return self._rpc({"op": "run", "argv": argv, "timeoutMs": 20000}, "run")

    def apps(self):
        self.identity()
        return self._rpc({"op": "apps"}, "apps")

    def restart(self):
        self.identity()
        return self._rpc({"op": "restart"}, "restart")
