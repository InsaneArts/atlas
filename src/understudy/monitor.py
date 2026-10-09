"""Loopback HTTP boundary for observation and explicit, leased input."""
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib.resources import files
import json
import os
import secrets
import socket
import threading
import time
from urllib.parse import urlsplit

from .control import ControlLease
from .errors import UnderstudyError
from .session import diagnose, epoch
from .transport import decode_json

ASSETS = {"": ("index.html", "text/html; charset=utf-8"),
          "app.js": ("app.js", "text/javascript; charset=utf-8"),
          "input.js": ("input.js", "text/javascript; charset=utf-8"),
          "preview.js": ("preview.js", "text/javascript; charset=utf-8"),
          "styles.css": ("styles.css", "text/css; charset=utf-8"),
          "favicon.svg": ("favicon.svg", "image/svg+xml")}
FRAME_LIFETIME = 8


class Viewer:
    def __init__(self, client):
        self.client = client
        self.lease = ControlLease(client.controller)
        self.lease_identity = None
        self.frames = {}
        self.frame_lock = threading.Lock()
        self.capture_lock = threading.Lock()

    def status(self):
        info = diagnose(self.client.status(), self.client.uid)
        if not info["ready"]:
            self.lease.release(force=True)
        info["controlActive"] = self.lease.active()
        return info

    def frame(self):
        # Serialize capture, but do not make input wait for PNG encoding.
        with self.capture_lock:
            current = epoch(self.client.identity())
            now = time.monotonic()
            with self.frame_lock:
                self.frames = {key: frame for key, frame in self.frames.items()
                               if now - frame.captured_at < FRAME_LIFETIME and frame.identity == current}
                if self.frames:
                    key = next(reversed(self.frames))
                    frame = self.frames[key]
                    if now - frame.captured_at < 0.4:
                        return key, frame
            frame = self.client.frame()
            if time.monotonic() - frame.captured_at >= FRAME_LIFETIME:
                raise UnderstudyError("Capture became stale while checking the session", "stale-frame")
            key = secrets.token_urlsafe(12)
            with self.frame_lock:
                self.frames[key] = frame
                while len(self.frames) > 24:
                    del self.frames[next(iter(self.frames))]
            return key, frame

    def observed_frame(self, key):
        with self.frame_lock:
            frame = self.frames.get(key) if isinstance(key, str) else None
        if frame is None or time.monotonic() - frame.captured_at >= FRAME_LIFETIME:
            raise UnderstudyError("The preview is stale. Wait for a fresh frame before controlling it.", "stale-frame")
        if frame.identity != epoch(self.client.identity()):
            raise UnderstudyError("The helper changed. Wait for a fresh frame and take control again.", "session-changed")
        return frame

    def control(self, route, body):
        if route == "control/release":
            return self.lease.release(body.get("lease"))
        if route != "control/acquire":
            with self.lease.mutex:
                self.lease.require(body.get("lease"))
        frame = self.observed_frame(body.get("frame"))
        if route == "control/acquire":
            state = diagnose(self.client.status(), self.client.uid)
            if not state["ready"]:
                raise UnderstudyError("Mouse and keyboard are not ready; check Session details", "permission")
            with self.lease.mutex:
                result = self.lease.acquire()
                self.lease_identity = frame.identity
                return result
        if frame.identity != self.lease_identity:
            raise UnderstudyError("The controlled session changed. Take control again.", "session-changed")
        if route == "control/renew":
            return self.lease.renew(body.get("lease"))
        if route == "input":
            with self.lease.action(body.get("lease")) as descriptor:
                result = self.client.input(body.get("actions"), lease=descriptor, expected_epoch=frame.identity,
                                           authorization=self.lease.dispatch(body.get("lease")))
                if result.get("ok") is not True:
                    self.lease.release(force=True)
                return result
        raise ValueError("Unknown operation")


class ViewerServer(ThreadingHTTPServer):
    daemon_threads = True

    def service_actions(self):
        self.viewer.lease.expire()


def create_server(client, port=8765):
    if type(port) is not int or not 0 <= port <= 65535:
        raise ValueError("Port must be an integer from 0 to 65535")
    viewer = Viewer(client)
    prefix = "/" + secrets.token_urlsafe(24) + "/"

    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.connection.settimeout(5)

        def log_message(self, *args):
            pass  # Capability URLs and input never enter an access log.

        def respond(self, code, content_type, body, headers=None):
            try:
                self.send_response(code)
                for name, value in {
                    "Content-Type": content_type, "Content-Length": str(len(body)),
                    "Cache-Control": "no-store", "Referrer-Policy": "no-referrer",
                    "X-Content-Type-Options": "nosniff",
                    "Content-Security-Policy": "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
                    **(headers or {}),
                }.items():
                    self.send_header(name, value)
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError, socket.timeout):
                pass

        def json(self, code, value):
            self.respond(code, "application/json", json.dumps(value).encode())

        def route(self, mutation=False):
            authority = "127.0.0.1:" + str(self.server.server_port)
            origin = "http://" + authority
            valid_origin = self.headers.get("Origin") == origin if mutation else self.headers.get("Origin") in (None, origin)
            if self.headers.get("Host") != authority or not valid_origin:
                self.json(403, {"ok": False, "error": "Local same-origin access only"})
                return None
            path = urlsplit(self.path).path
            if not path.startswith(prefix):
                self.json(404, {"ok": False, "error": "Use the viewer URL printed by Understudy"})
                return None
            return path[len(prefix):]

        def do_GET(self):
            route = self.route()
            if route is None:
                return
            try:
                if route in ASSETS:
                    filename, mime = ASSETS[route]
                    self.respond(200, mime, files("understudy").joinpath("web").joinpath(filename).read_bytes())
                elif route == "status.json":
                    self.json(200, viewer.status())
                elif route == "frame.png":
                    key, frame = viewer.frame()
                    self.respond(200, "image/png", frame.png, {
                        "X-Frame-Id": key, "X-Display-Width": str(frame.display["width"]),
                        "X-Display-Height": str(frame.display["height"]),
                        "X-Session-Revision": ":".join(str(v) for v in frame.identity),
                    })
                else:
                    self.json(404, {"ok": False, "error": "Unknown view"})
            except (UnderstudyError, OSError, ValueError) as exc:
                viewer.lease.release(force=True)
                self.json(503, {"ok": False, "error": str(exc)})

        def do_POST(self):
            body = {}
            route = self.route(mutation=True)
            if route is None:
                return
            if route not in {"control/acquire", "control/renew", "control/release", "input"}:
                self.json(404, {"ok": False, "error": "Unknown operation"})
                return
            try:
                if self.headers.get("Transfer-Encoding") or self.headers.get_content_type() != "application/json":
                    raise ValueError("Send a JSON request with Content-Length")
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 32768:
                    raise ValueError("Request body must contain 1–32768 bytes")
                raw = self.rfile.read(length)
                if len(raw) != length:
                    raise ValueError("Incomplete request body")
                body = decode_json(raw)
                if not isinstance(body, dict):
                    raise ValueError("Request must be a JSON object")
                result = viewer.control(route, body)
                self.json(200 if result.get("ok") is True else 409, result)
            except UnderstudyError as exc:
                if exc.code not in {"busy", "lease-lost"}:
                    viewer.lease.release_if_owner(body.get("lease"))
                self.json(409, exc.as_dict())
            except (ValueError, OSError) as exc:
                self.json(400, {"ok": False, "error": str(exc)})

    server = ViewerServer(("127.0.0.1", port), Handler)
    server.viewer = viewer
    server.url = "http://127.0.0.1:" + str(server.server_port) + prefix
    return server


def serve(client, port=8765):
    server = create_server(client, port)
    record = {"url": server.url, "pid": os.getpid(), "control": "explicit lease",
              "startedAt": datetime.now(timezone.utc).isoformat()}
    client.artifacts.write("monitor.json", (json.dumps(record, indent=2) + "\n").encode())
    print(json.dumps(record, indent=2), flush=True)
    print("Open this URL in your browser. Ctrl+C stops the viewer.", flush=True)
    try:
        server.serve_forever(poll_interval=0.25)
    except KeyboardInterrupt:
        pass
    finally:
        viewer = server.viewer
        viewer.lease.release(force=True)
        server.server_close()
    return 0
