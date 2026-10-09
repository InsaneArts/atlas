"""Isolated boundary regressions, not substitutes for real desktop evidence."""
from contextlib import contextmanager
import copy
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from understudy.actions import validate_actions
from understudy.client import Client
from understudy.config import Config
from understudy.control import ControlLease, ControllerLock
from understudy.errors import SessionError, UnderstudyError
from understudy.session import diagnose, require_hidden_session
from understudy.transport import Transport

STATE = {"ok": True, "daemon": {"pid": 123}, "user": {"uid": 502, "name": "helper"},
         "session": {"onConsole": False, "managerName": "Aqua"},
         "display": {"width": 2560, "height": 1440, "scale": 1},
         "permissions": {"screenCapture": True, "accessibility": True}}


@contextmanager
def peer(chunks, delay=0, **limits):
    """A real isolated socket to exercise protocol framing and deadlines."""
    with tempfile.TemporaryDirectory(prefix="ust-", dir="/tmp") as directory:
        path = str(Path(directory) / "rpc.sock")
        server = socket.socket(socket.AF_UNIX)
        server.bind(path)
        server.listen()
        server.settimeout(.5)
        requests = []

        def answer():
            try:
                connection, _ = server.accept()
                with connection:
                    requests.append(connection.recv(65536))
                    for chunk in chunks:
                        time.sleep(delay)
                        connection.sendall(chunk)
            except OSError:
                pass

        worker = threading.Thread(target=answer, daemon=True)
        worker.start()
        try:
            yield Transport(path, **limits), requests
        finally:
            server.close()
            worker.join(timeout=2)


class TransportTests(unittest.TestCase):
    def test_fragmented_real_socket_exchange(self):
        with peer([b'{"event":"started","pid":2}\n{"ok":', b'true}\n']) as (transport, requests):
            result = transport.call({"op": "hello"})
            self.assertTrue(result["ok"])
            self.assertEqual(result["events"][0]["pid"], 2)
            self.assertEqual(len(requests), 1)

    def test_malformed_replies_preserve_mutation_uncertainty(self):
        for data in [b'null\n', b'12\n', b'["ok"]\n', b'{"ok":1}\n',
                     b'{"ok":true,"x":1e10000}\n', b'[' * 1500 + b']' * 1500 + b'\n']:
            with self.subTest(data=data[:30]), peer([data]) as (transport, _):
                with self.assertRaises(UnderstudyError) as failure:
                    transport.call({"op": "input", "actions": []})
                self.assertTrue(failure.exception.uncertain)

    def test_reply_size_and_event_limits(self):
        for chunks, limits in [([b' ' * 200], {"max_bytes": 100}),
                               ([b'{"event":"output"}\n' * 3], {"max_events": 2})]:
            with peer(chunks, **limits) as (transport, _), self.assertRaises(UnderstudyError):
                transport.call({"op": "hello"})

    def test_total_deadline_despite_continuous_bytes(self):
        started = time.monotonic()
        with peer([b' '] * 20, delay=.02, timeout=.08) as (transport, _):
            with self.assertRaises(UnderstudyError):
                transport.call({"op": "hello"})
        self.assertLess(time.monotonic() - started, .5)


class ConfigAndPolicyTests(unittest.TestCase):
    def test_invalid_configs_fail_cleanly(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.json"
            for value in [None, [], {"helper_uid": 502, "socket_path": 12},
                          {"helper_uid": 502, "socket_path": "sock", "output_dir": None}]:
                path.write_text(json.dumps(value))
                with self.subTest(value=value), self.assertRaises((ValueError, SessionError)):
                    Config.load(path)
            path.write_text('[' * 1500 + ']' * 1500)
            with self.assertRaises(ValueError):
                Config.load(path)

    def test_paths_resolve_relative_to_config(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.json"
            path.write_text('{"helper_uid":502,"socket_path":"socket","output_dir":"output"}')
            config = Config.load(path)
            self.assertEqual(config.socket_path, path.parent.resolve() / "socket")
            self.assertEqual(config.output_dir, path.parent.resolve() / "output")

    def test_observation_does_not_require_input_permission(self):
        state = copy.deepcopy(STATE)
        state["permissions"]["accessibility"] = False
        result = diagnose(state, 502)
        self.assertTrue(result["canObserve"])
        self.assertFalse(result["ready"])
        self.assertEqual(len(result["nextSteps"]), 1)
        self.assertFalse(diagnose({"ok": False}, 502)["canObserve"])

    def test_extreme_identity_fails_closed(self):
        state = copy.deepcopy(STATE)
        state["display"]["width"] = 10 ** 1000
        with self.assertRaises(SessionError):
            require_hidden_session(state, 502)


class ActionTests(unittest.TestCase):
    def test_unsafe_numbers_rejected(self):
        for value in [True, -1, float('nan'), float('inf'), 10 ** 1000]:
            with self.subTest(value=str(value)[:20]), self.assertRaises(ValueError):
                validate_actions([{"type": "click", "x": value, "y": 20}], STATE["display"])
        for action in [{"type": "scroll", "x": 10, "y": 10, "dy": 1.5},
                       {"type": "scroll", "x": 10, "y": 10, "dy": 2**64},
                       {"type": "click", "x": 10, "y": 10, "count": 0},
                       {"type": "key", "key": "command+invented"},
                       {"type": "wait", "ms": 5000}, {"type": "type", "text": "x" * 257}]:
            with self.subTest(action=action), self.assertRaises(ValueError):
                validate_actions([action], STATE["display"])

    def test_entire_batch_bounded_before_dispatch(self):
        with self.assertRaises(ValueError):
            validate_actions([{"type": "wait", "ms": 250}] * 9, STATE["display"])
        with self.assertRaises(ValueError):
            validate_actions([{"type": "key", "key": "return"}, {"type": "run"}], STATE["display"])


class OwnershipTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.lock = ControllerLock(502, self.root / "socket", directory=self.root)

    def tearDown(self):
        self.temp.cleanup()

    def test_aliases_and_other_process_share_lock(self):
        alias = self.root / "alias"
        alias.symlink_to(self.root)
        other = ControllerLock(502, alias / "socket", directory=self.root)
        self.assertEqual(self.lock.path, other.path)
        with self.lock.held():
            source = 'from understudy.control import ControllerLock; from understudy.errors import UnderstudyError; import sys\ntry:\n ControllerLock(502,sys.argv[1],sys.argv[2]).acquire()\nexcept UnderstudyError as e:\n print(e.code)\n'
            env = dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parents[1] / "src"))
            result = subprocess.run([sys.executable, '-c', source, str(alias / 'socket'), str(self.root)], env=env, capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout.strip(), "busy")

    def test_expiry_retains_lock_until_inflight_finishes(self):
        now = [0.0]
        lease = ControlLease(self.lock, ttl=1, clock=lambda: now[0])
        token = lease.acquire()["lease"]
        with lease.action(token):
            now[0] = 2
            lease.expire()
            with self.assertRaises(UnderstudyError):
                self.lock.acquire()
            with self.assertRaises(UnderstudyError):
                lease.renew(token)
        with self.lock.held():
            pass

    def test_wrong_and_unicode_owner_cannot_release(self):
        lease = ControlLease(self.lock)
        token = lease.acquire()["lease"]
        for wrong in [None, "wrong", "日本語"]:
            with self.assertRaises(UnderstudyError):
                lease.renew(wrong)
            lease.release_if_owner(wrong)
            self.assertTrue(lease.active())
        lease.release(token)


class ClientOutcomeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.client = Client(502, Path(self.temp.name) / 'socket', Path(self.temp.name) / 'runs')
        self.client.controller = ControllerLock(502, self.client.socket_path, directory=self.temp.name)
        self.client.identity = lambda: copy.deepcopy(STATE)

    def tearDown(self):
        self.temp.cleanup()

    def test_interrupt_leaves_pending_marker_and_blocks_replay(self):
        with patch.object(self.client.transport, 'call', side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.client.input([{"type": "type", "text": "secret-input"}])
        with self.assertRaises(UnderstudyError):
            with self.client.controller.held():
                pass

    def test_failed_recovery_does_not_clear_prior_uncertainty(self):
        self.client.controller.quarantine()
        with patch.object(self.client.transport, 'call', side_effect=UnderstudyError('not connected', 'transport')):
            with self.assertRaises(UnderstudyError):
                self.client.restart()
        self.assertTrue(self.client.controller.uncertain_path.exists())

    def test_release_during_preflight_prevents_dispatch(self):
        lease = ControlLease(self.client.controller)
        token = lease.acquire()["lease"]
        entered, resume = threading.Event(), threading.Event()
        failures = []

        def delayed_identity():
            entered.set()
            self.assertTrue(resume.wait(1))
            return copy.deepcopy(STATE)

        self.client.identity = delayed_identity
        with peer([]) as (transport, requests):
            self.client.transport = transport

            def action():
                try:
                    with lease.action(token) as descriptor:
                        self.client.input([{"type": "key", "key": "escape"}], lease=descriptor,
                                          authorization=lease.dispatch(token))
                except UnderstudyError as exc:
                    failures.append(exc.code)

            worker = threading.Thread(target=action)
            worker.start()
            self.assertTrue(entered.wait(1))
            lease.release(token)
            resume.set()
            worker.join(timeout=1)
            self.assertFalse(worker.is_alive())
            self.assertEqual(failures, ['lease-lost'])
            self.assertEqual(requests, [])

    def test_logging_failure_preserves_acknowledged_input(self):
        (self.client.output_dir / 'metadata.jsonl').mkdir()
        with patch.object(self.client.transport, 'call', return_value={"ok": True, "performed": 1}):
            result = self.client.input([{"type": "key", "key": "escape"}])
        self.assertTrue(result["ok"])
        self.assertTrue(result["warnings"])

    def test_journal_contains_no_text_argv_or_output(self):
        with patch.object(self.client.transport, 'call', return_value={"ok": True, "performed": 1}):
            self.client.input([{"type": "type", "text": "synthetic-private-input"}])
        log = (self.client.output_dir / 'metadata.jsonl').read_text()
        self.assertNotIn('synthetic-private-input', log)
        self.assertNotIn('actions', log)
        self.assertEqual(json.loads(log)["actionCount"], 1)

    def test_malformed_success_stays_uncertain(self):
        with patch.object(self.client.transport, 'call', return_value={"ok": True}):
            with self.assertRaises(UnderstudyError) as failure:
                self.client.run(['test'])
        self.assertTrue(failure.exception.uncertain)
        self.assertTrue(self.client.controller.uncertain_path.exists())

    def test_capture_requires_actual_png_payload(self):
        for reply in [{"ok": True}, {"ok": True, "png": 3}, {"ok": True, "png": "bm90LXBuZw=="}]:
            with self.subTest(reply=reply), patch.object(self.client.transport, 'call', return_value=reply):
                with self.assertRaises(SessionError):
                    self.client.frame()


if __name__ == '__main__':
    unittest.main()
