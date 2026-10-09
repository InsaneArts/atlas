"""Explicit captures and bounded, payload-free operational metadata."""
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import time


class Artifacts:
    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.directory.chmod(0o700)

    def write(self, name, data):
        path = self.directory / name
        flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW
        with os.fdopen(os.open(path, flags, 0o600), "wb") as stream:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(data)
        return path

    def capture(self, png):
        return self.write(str(time.time_ns()) + "-capture.png", png).resolve()

    def record(self, operation, result, elapsed_ms, action_count=None):
        # Never record typed text, argv, output events, window titles or frames.
        record = {"at": datetime.now(timezone.utc).isoformat(), "operation": operation,
                  "ok": result.get("ok") is True, "elapsedMs": round(elapsed_ms, 3)}
        code = result.get("code")
        if isinstance(code, str) and len(code) <= 64:
            record["code"] = code
        if action_count is not None:
            record["actionCount"] = action_count
        payload = (json.dumps(record) + "\n").encode()
        lock_path = self.directory / "metadata.lock"
        with os.fdopen(os.open(lock_path, os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW, 0o600), "wb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            path = self.directory / "metadata.jsonl"
            if path.exists() and path.stat().st_size + len(payload) > 1024 * 1024:
                path.replace(self.directory / "metadata.previous.jsonl")
            with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600), "ab") as stream:
                os.fchmod(stream.fileno(), 0o600)
                stream.write(payload)
