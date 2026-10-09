"""Guarded operations composed from transport, policy, ownership and artifacts."""
import base64
from contextlib import nullcontext
from dataclasses import dataclass
from pathlib import Path
import struct
import time

from .actions import validate_actions
from .artifacts import Artifacts
from .control import ControllerLock
from .errors import SessionError, UnderstudyError
from .session import epoch, require_hidden_session, validate_helper_uid
from .transport import Transport


@dataclass(frozen=True)
class Frame:
    png: bytes
    width: int
    height: int
    display: dict
    identity: tuple
    captured_at: float


class Client:
    def __init__(self, uid, socket_path, output_dir):
        validate_helper_uid(uid)
        self.uid = uid
        self.socket_path = Path(socket_path).expanduser().resolve()
        self.artifacts = Artifacts(output_dir)
        self.output_dir = self.artifacts.directory
        self.transport = Transport(self.socket_path)
        self.controller = ControllerLock(uid, self.socket_path)

    def _call(self, request, authorization=None):
        started = time.monotonic()
        mutation = request["op"] in {"input", "run", "restart"}
        # Persist before dispatch: interruption/SIGKILL cannot make an unfinished
        # native action appear idle merely because this process lost its flock.
        if mutation:
            self.controller.quarantine()
        try:
            result = self.transport.call(request, authorization=authorization)
        except UnderstudyError as exc:
            result = exc.as_dict()
            if mutation and request["op"] != "restart" and not exc.uncertain:
                self.controller.clear_quarantine()
            self._record(request, result, started)
            raise
        except ValueError:
            if mutation and request["op"] != "restart":
                self.controller.clear_quarantine()
            raise
        if request["op"] == "input" and result.get("ok") is True and (
                type(result.get("performed")) is not int or result["performed"] != len(request["actions"])):
            raise UnderstudyError("Helper did not confirm the input count", "protocol", uncertain=True)
        if request["op"] == "run" and result.get("ok") is True and (
                type(result.get("timedOut")) is not bool or "exitCode" not in result
                or (result["exitCode"] is not None and type(result["exitCode"]) is not int)
                or (result["exitCode"] is None and not result["timedOut"] and type(result.get("signal")) is not int)):
            raise UnderstudyError("Helper did not confirm command completion", "protocol", uncertain=True)
        if mutation and request["op"] != "restart":
            try:
                self.controller.clear_quarantine()
            except OSError:
                result.setdefault("warnings", []).append("Operation completed, but its pending marker could not be cleared")
        self._record(request, result, started)
        return result

    def _record(self, request, result, started):
        try:
            self.artifacts.record(request["op"], result, (time.monotonic() - started) * 1000,
                                  len(request["actions"]) if "actions" in request else None)
        except OSError:
            result.setdefault("warnings", []).append("Local metadata could not be recorded; the operation result remains valid")

    def status(self):
        try:
            return self._call({"op": "hello"})
        except UnderstudyError as exc:
            return exc.as_dict()

    def identity(self):
        return require_hidden_session(self.status(), self.uid)

    def frame(self, maximum=1600):
        if maximum is not None and (type(maximum) is not int or not 1 <= maximum <= 8192):
            raise ValueError("max-dimension must be an integer from 1 to 8192")
        before = self.identity()
        request = {"op": "screenshot"}
        if maximum is not None:
            request["maxDimension"] = maximum
        reply = self._call(request)
        captured_at = time.monotonic()
        if reply.get("ok") is not True:
            raise SessionError(reply.get("error", "Capture failed"), reply.get("code", "capture"))
        try:
            encoded = reply.get("png")
            if not isinstance(encoded, str):
                raise ValueError("Missing PNG payload")
            png = base64.b64decode(encoded, validate=True)
            if len(png) < 33 or png[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
                raise ValueError("Invalid PNG header")
            width, height = struct.unpack(">II", png[16:24])
            if not (0 < width <= 32768 and 0 < height <= 32768) or (width, height) != (reply.get("width"), reply.get("height")):
                raise ValueError("PNG dimensions do not match the reply")
        except (ValueError, TypeError) as exc:
            raise SessionError("Helper returned an invalid image: " + str(exc), "capture") from exc
        after = self.identity()
        if epoch(before) != epoch(after):
            raise SessionError("Session changed during capture; observe again", "session-changed")
        return Frame(png, width, height, dict(after["display"]), epoch(after), captured_at)

    def capture(self, maximum=None):
        frame = self.frame(maximum)
        path = self.artifacts.capture(frame.png)
        return {"ok": True, "path": str(path), "width": frame.width, "height": frame.height,
                "display": frame.display}

    def input(self, actions, lease=None, expected_epoch=None, authorization=None):
        ownership = self.controller.held() if lease is None else nullcontext(lease)
        with ownership:
            self.controller.require_known_delivery()
            state = self.identity()
            if expected_epoch is not None and epoch(state) != expected_epoch:
                raise SessionError("Session changed since this frame; observe again", "session-changed")
            permissions = state.get("permissions")
            if not isinstance(permissions, dict) or permissions.get("accessibility") is not True:
                raise SessionError("The helper needs Accessibility permission", "permission")
            validate_actions(actions, state["display"])
            return self._call({"op": "input", "actions": actions}, authorization=authorization)

    def run(self, argv):
        if not isinstance(argv, list) or not argv or any(not isinstance(a, str) or "\0" in a for a in argv) or not argv[0]:
            raise ValueError("Provide a command and string arguments after run --")
        with self.controller.held():
            self.identity()
            return self._call({"op": "run", "argv": argv, "timeoutMs": 20000})

    def apps(self):
        self.identity()
        return self._call({"op": "apps"})

    def restart(self):
        with self.controller.held(recovery=True):
            previous = epoch(self.identity())
            reply = self._call({"op": "restart"})
            if reply.get("ok") is not True:
                return reply
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                time.sleep(0.15)
                try:
                    state = self.identity()
                    if epoch(state)[1] != previous[1]:
                        self.controller.clear_quarantine()
                        return {"ok": True, "restarted": True, "session": state}
                except SessionError:
                    continue
            self.controller.quarantine()
            raise UnderstudyError("Restart was acknowledged but a new helper was not confirmed", "restart", uncertain=True)
