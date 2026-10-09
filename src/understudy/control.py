"""Cooperative controller ownership. Raw daemon callers can bypass this lock."""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import threading
import time

from .errors import UnderstudyError


class ControllerLock:
    def __init__(self, uid, socket_path, directory=None):
        root = Path(directory) if directory else Path.home() / "Library/Caches/Understudy"
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
        key = hashlib.sha256((str(uid) + ":" + str(Path(socket_path).resolve())).encode()).hexdigest()[:24]
        self.path = root / (key + ".lock")
        self.uncertain_path = root / (key + ".uncertain.json")

    def acquire(self):
        descriptor = os.open(self.path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            os.close(descriptor)
            raise UnderstudyError("Another Understudy controller is active. Release it before taking control.", "busy") from exc
        return descriptor

    @staticmethod
    def release(descriptor):
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)

    def require_known_delivery(self):
        if self.uncertain_path.exists():
            raise UnderstudyError("The last action's outcome is unknown. Observe the desktop, then run 'understudy restart' before further control.", "uncertain")

    def quarantine(self):
        descriptor = os.open(self.uncertain_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "w") as stream:
            json.dump({"delivery": "unknown"}, stream)

    def clear_quarantine(self):
        self.uncertain_path.unlink(missing_ok=True)

    @contextmanager
    def held(self, recovery=False):
        descriptor = self.acquire()
        try:
            if not recovery:
                self.require_known_delivery()
            yield descriptor
        finally:
            self.release(descriptor)


class ControlLease:
    """A per-tab lease. Invalidation is immediate; in-flight work retains flock."""
    def __init__(self, controller, ttl=10, clock=time.monotonic):
        self.controller = controller
        self.ttl = ttl
        self.clock = clock
        self.mutex = threading.RLock()
        self.token = None
        self.descriptor = None
        self.expires = 0
        self.inflight = 0

    def _release_if_idle(self):
        if self.token is None and not self.inflight and self.descriptor is not None:
            self.controller.release(self.descriptor)
            self.descriptor = None

    def expire(self):
        with self.mutex:
            if self.token and self.clock() >= self.expires:
                self.token = None
            self._release_if_idle()

    def acquire(self):
        with self.mutex:
            self.expire()
            if self.descriptor is not None:
                raise UnderstudyError("Another viewer has control", "busy")
            self.descriptor = self.controller.acquire()
            try:
                self.controller.require_known_delivery()
            except BaseException:
                self.controller.release(self.descriptor)
                self.descriptor = None
                raise
            self.token = secrets.token_urlsafe(24)
            self.expires = self.clock() + self.ttl
            return {"ok": True, "lease": self.token, "ttlSeconds": self.ttl}

    def require(self, token):
        self.expire()
        if not isinstance(token, str) or not token.isascii() or not self.token or not secrets.compare_digest(token, self.token):
            raise UnderstudyError("Control was released or expired. Take control again.", "lease-lost")

    def renew(self, token):
        with self.mutex:
            self.require(token)
            self.expires = self.clock() + self.ttl
            return {"ok": True}

    def release(self, token=None, force=False):
        with self.mutex:
            if not force:
                self.require(token)
            self.token = None
            self._release_if_idle()
            return {"ok": True}

    @contextmanager
    def action(self, token):
        with self.mutex:
            self.require(token)
            if self.inflight:
                raise UnderstudyError("Wait for the current input to finish", "busy")
            self.inflight += 1
        try:
            yield self.descriptor
        finally:
            with self.mutex:
                self.inflight -= 1
                self.expire()

    @contextmanager
    def dispatch(self, token):
        # Release can cancel preflight. Once bytes have been submitted, the
        # native operation may finish; the action context retains its flock.
        with self.mutex:
            self.require(token)
            yield

    def active(self):
        with self.mutex:
            self.expire()
            return self.descriptor is not None

    def release_if_owner(self, token):
        with self.mutex:
            if isinstance(token, str) and token.isascii() and self.token and secrets.compare_digest(token, self.token):
                self.release(force=True)
