"""Bounded JSONL transport with one wall-clock deadline and no retries."""
import json
import math
from contextlib import nullcontext
import socket
import time

from .errors import UnderstudyError


def decode_json(data):
    def finite_float(value):
        result = float(value)
        if not math.isfinite(result):
            raise ValueError("Non-finite JSON number")
        return result

    def bounded_integer(value):
        if len(value) > 32:
            raise ValueError("JSON integer exceeds the protocol range")
        return int(value)

    def invalid_constant(value):
        raise ValueError("Invalid JSON constant: " + value)

    try:
        return json.loads(data, parse_float=finite_float, parse_int=bounded_integer,
                          parse_constant=invalid_constant)
    except RecursionError as exc:
        raise ValueError("JSON nesting exceeds the protocol limit") from exc


class Transport:
    def __init__(self, path, timeout=30, max_bytes=64 * 1024 * 1024, max_events=1024):
        self.path = str(path)
        self.timeout = timeout
        self.max_bytes = max_bytes
        self.max_events = max_events

    def call(self, request, authorization=None):
        payload = (json.dumps(request, allow_nan=False) + "\n").encode()
        if len(payload) > 1024 * 1024:
            raise ValueError("Request exceeds the helper's 1 MiB limit")
        started = time.monotonic()
        sent = False
        mutation = request.get("op") in {"input", "run", "restart"}
        try:
            with socket.socket(socket.AF_UNIX) as connection:
                def remaining():
                    left = self.timeout - (time.monotonic() - started)
                    if left <= 0:
                        raise TimeoutError("Helper exceeded the request deadline")
                    connection.settimeout(left)

                with authorization if authorization is not None else nullcontext():
                    remaining()
                    connection.connect(self.path)
                    remaining()
                    sent = True  # A failed sendall can still have delivered a prefix.
                    connection.sendall(payload)
                pending = bytearray()
                scanned = 0
                received = 0
                events = []
                while True:
                    remaining()
                    chunk = connection.recv(min(65536, self.max_bytes - received + 1))
                    if not chunk:
                        raise ValueError("Connection ended without a final reply")
                    received += len(chunk)
                    if received > self.max_bytes:
                        raise ValueError("Helper reply exceeded the size limit")
                    pending.extend(chunk)
                    while True:
                        newline = pending.find(b"\n", scanned)
                        if newline < 0:
                            scanned = len(pending)
                            break
                        line = pending[:newline]
                        del pending[:newline + 1]
                        scanned = 0
                        row = decode_json(line)
                        if not isinstance(row, dict):
                            raise ValueError("Helper reply must be a JSON object")
                        if "ok" in row:
                            if type(row["ok"]) is not bool:
                                raise ValueError("Helper reply has an invalid success flag")
                            if events:
                                row["events"] = events
                            row["elapsedMs"] = (time.monotonic() - started) * 1000
                            return row
                        if len(events) >= self.max_events:
                            raise ValueError("Helper emitted too many events")
                        if not isinstance(row.get("event"), str):
                            raise ValueError("Helper event has no event type")
                        events.append(row)
        except (OSError, ValueError, UnicodeError) as exc:
            raise UnderstudyError(str(exc), "transport", uncertain=sent and mutation) from exc
