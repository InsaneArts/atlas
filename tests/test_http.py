"""Isolated HTTP/lease boundary tests. Live browser input has its own harness."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))
from understudy.client import Frame
from understudy.control import ControllerLock
from understudy.monitor import create_server
from understudy.session import epoch
from test_boundaries import STATE


class BoundaryClient:
    def __init__(self, directory):
        self.uid = 502
        self.controller = ControllerLock(502, Path(directory) / 'socket', directory=directory)

    def identity(self):
        return copy.deepcopy(STATE)

    status = identity

    def frame(self):
        return Frame(b'isolated HTTP fixture, not desktop pixels', 1600, 900,
                     STATE['display'], epoch(STATE), time.monotonic())


class HTTPTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.server = create_server(BoundaryClient(self.temp.name), 0)
        self.thread = threading.Thread(target=lambda: self.server.serve_forever(poll_interval=.01), daemon=True)
        self.thread.start()
        self.url = self.server.url
        self.origin = self.url.split('/', 3)[:3]
        self.origin = '/'.join(self.origin)

    def tearDown(self):
        self.server.viewer.lease.release(force=True)
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.temp.cleanup()

    def request(self, route, body=None, headers=None, url=None):
        fields = {} if body is None else {'Content-Type': 'application/json', 'Origin': self.origin}
        fields.update(headers or {})
        data = None if body is None else json.dumps(body).encode()
        request = Request(url or self.url + route, data=data, headers=fields)
        try:
            response = urlopen(request, timeout=1)
        except HTTPError as exc:
            response = exc
        with response:
            return response.status, response.headers, response.read()

    def acquire(self):
        code, headers, _ = self.request('frame.png')
        self.assertEqual(code, 200)
        frame = headers['X-Frame-Id']
        code, _, data = self.request('control/acquire', {'frame': frame})
        self.assertEqual(code, 200)
        return frame, json.loads(data)['lease']

    def test_capability_origin_and_body_boundaries(self):
        self.assertEqual(self.request('', url=self.origin+'/')[0], 404)
        self.assertEqual(self.request('control/acquire', {}, {'Origin': 'https://invalid.example'})[0], 403)
        self.assertEqual(self.request('control/acquire', {})[0], 409)
        self.assertEqual(self.request('control/acquire', [1, 2])[0], 400)
        self.assertEqual(self.request('run', {})[0], 404)
        self.assertEqual(self.request('control/acquire', {'padding': 'x' * 33000})[0], 400)

    def test_bad_owner_cannot_cancel_current_controller(self):
        frame, owner = self.acquire()
        for token in ['wrong', '日本語']:
            code, _, body = self.request('input', {'lease': token, 'frame': 'stale', 'actions': []})
            self.assertEqual(code, 409)
            self.assertEqual(json.loads(body)['code'], 'lease-lost')
            self.assertTrue(self.server.viewer.lease.active())
        self.assertEqual(self.request('control/renew', {'lease': owner, 'frame': frame})[0], 200)

    def test_stale_frame_releases_its_owner_only(self):
        _, owner = self.acquire()
        code, _, body = self.request('input', {'lease': owner, 'frame': 'stale', 'actions': []})
        self.assertEqual(code, 409)
        self.assertEqual(json.loads(body)['code'], 'stale-frame')
        self.assertFalse(self.server.viewer.lease.active())

    def test_abandoned_lease_expires_without_another_request(self):
        self.server.viewer.lease.ttl = .04
        self.acquire()
        time.sleep(.1)
        with self.server.viewer.client.controller.held():
            pass


if __name__ == '__main__':
    unittest.main()
