// The live checks' viewer client, against a stand-in for the viewer service.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { ViewerClient, recordPath, textChunks } from '../scripts/viewer-client.mjs';

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function until(condition, limit = 2000) {
  const deadline = Date.now() + limit;
  while (!condition() && Date.now() < deadline) await wait(10);
}

/** A local service that records every request and answers like the real one. */
async function stub(answer = () => null) {
  const requests = [];
  let frames = 0;
  const server = http.createServer((request, response) => {
    let text = '';
    request.on('data', (chunk) => (text += chunk));
    request.on('end', () => {
      const route = request.url.replace(/^\/secret\//, '');
      const body = text ? JSON.parse(text) : null;
      requests.push({ method: request.method, route, headers: request.headers, body });
      const reply = (status, value) => {
        response.writeHead(status, { 'Content-Type': 'application/json' });
        response.end(JSON.stringify(value));
      };
      const custom = answer(route, body);
      // A reply that never arrives.
      if (custom === 'drop') return request.socket.destroy();
      if (custom) return reply(...custom);
      if (route === 'frame.png') {
        frames++;
        response.writeHead(200, {
          'Content-Type': 'image/png',
          'X-Frame-Id': `frame-${frames}`,
          'X-Session-Revision': 'revision',
          'X-Display-Width': '2560',
          'X-Display-Height': '1440',
        });
        return response.end(Buffer.from([0x89, 0x50, 0x4e, 0x47]));
      }
      if (route === 'status.json') return reply(200, { ok: true, ready: true });
      if (route === 'control/acquire') return reply(200, { ok: true, lease: 'lease-1' });
      reply(200, { ok: true });
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}/secret/`;
  return { url, requests, close: () => new Promise((resolve) => server.close(resolve)) };
}

const posts = (requests, route) =>
  requests.filter((request) => request.method === 'POST' && request.route === route);

test('reads the URL from a record, or a folder holding menubar.json', async () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-client-'));
  try {
    const record = path.join(folder, 'menubar.json');
    fs.writeFileSync(record, JSON.stringify({ url: 'http://127.0.0.1:1234/abc/' }));
    assert.equal(recordPath(folder), record);
    assert.equal(recordPath(undefined, record), record);
    const client = new ViewerClient(record);
    assert.equal(client.url, 'http://127.0.0.1:1234/abc/');
    assert.equal(client.origin, 'http://127.0.0.1:1234');
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

test('posts carry the service origin and JSON, and frames their geometry', async () => {
  const service = await stub();
  const client = new ViewerClient(service.url);
  try {
    const frame = await client.frame();
    assert.deepEqual(
      { id: frame.id, revision: frame.revision, width: frame.width, height: frame.height },
      { id: 'frame-1', revision: 'revision', width: 2560, height: 1440 },
    );
    assert.equal(frame.png.length, 4);
    assert.equal(await client.acquire(), 'lease-1');
    await client.release();
    for (const request of service.requests.filter((request) => request.method === 'POST')) {
      assert.equal(request.headers.origin, client.origin);
      assert.equal(request.headers['content-type'], 'application/json');
      assert.equal(
        Number(request.headers['content-length']),
        Buffer.byteLength(JSON.stringify(request.body)),
      );
      assert.equal(request.headers['transfer-encoding'], undefined);
    }
    assert.deepEqual(posts(service.requests, 'control/acquire')[0].body, { frame: 'frame-2' });
    assert.deepEqual(posts(service.requests, 'control/release')[0].body, { lease: 'lease-1' });
  } finally {
    client.stop();
    await service.close();
  }
});

test('a raw post can claim another origin, and refusals keep their code', async () => {
  const service = await stub((route) =>
    route === 'input' ? [409, { ok: false, error: 'Control was lost', code: 'lease-lost' }] : null,
  );
  const client = new ViewerClient(service.url);
  try {
    const refused = await client.post('input', { actions: [] }, { origin: 'https://x.example' });
    assert.equal(refused.status, 409);
    assert.equal(refused.body.code, 'lease-lost');
    assert.equal(posts(service.requests, 'input')[0].headers.origin, 'https://x.example');
    await client.acquire();
    await assert.rejects(client.key('escape'), { status: 409, code: 'lease-lost' });
  } finally {
    client.stop();
    await service.close();
  }
});

test('input names the latest frame, and the keep-alive renews with it', async () => {
  const service = await stub();
  const client = new ViewerClient(service.url, { frameEvery: 20, renewEvery: 50 });
  try {
    await client.acquire();
    await until(
      () => posts(service.requests, 'control/renew').length >= 2 && client.latest.id !== 'frame-1',
    );
    const latest = client.latest.id;
    await client.key('cmd+a');
    const input = posts(service.requests, 'input')[0].body;
    assert.equal(input.lease, 'lease-1');
    assert.equal(input.frame, latest);
    assert.notEqual(input.frame, 'frame-1');
    const renewals = posts(service.requests, 'control/renew');
    assert.ok(renewals.length >= 2, `${renewals.length} renewals`);
    assert.ok(renewals.every((request) => request.body.lease === 'lease-1'));
    assert.notEqual(renewals.at(-1).body.frame, 'frame-1');
  } finally {
    client.stop();
    await service.close();
  }
});

test('release stops the keep-alive', async () => {
  const service = await stub();
  const client = new ViewerClient(service.url, { frameEvery: 20, renewEvery: 30 });
  try {
    await client.acquire();
    await wait(80);
    await client.release();
    // Anything already in flight lands first.
    await wait(30);
    const count = service.requests.length;
    await wait(120);
    assert.equal(service.requests.length, count);
    await assert.rejects(client.key('escape'), /Take control first/);
  } finally {
    client.stop();
    await service.close();
  }
});

test('a refused renewal ends control and is the next error', async () => {
  const service = await stub((route) =>
    route === 'control/renew'
      ? [409, { ok: false, error: 'Control expired', code: 'lease-lost' }]
      : null,
  );
  const client = new ViewerClient(service.url, { frameEvery: 1000, renewEvery: 20 });
  try {
    await client.acquire();
    await until(() => client.lease === null);
    assert.equal(client.lease, null);
    await assert.rejects(client.key('escape'), { code: 'lease-lost' });
  } finally {
    client.stop();
    await service.close();
  }
});

test('a lost reply to a post is an unknown outcome', async () => {
  const client = new ViewerClient('http://127.0.0.1:9/secret/', { timeout: 500 });
  await assert.rejects(client.call('recover', {}), { uncertain: true });
});

test('text chunks never split a grapheme', () => {
  const family = '👩‍👩‍👧‍👦';
  const chunks = textChunks('a'.repeat(126) + family + 'b', 128);
  assert.deepEqual(chunks, ['a'.repeat(126), family + 'b']);
  assert.deepEqual(textChunks('é'.repeat(3), 3), ['é', 'é', 'é']);
  assert.deepEqual(textChunks(''), []);
  const long = 'x' + '́'.repeat(300);
  assert.deepEqual(textChunks(long), [long]);
});

test('typing sends one batch per chunk and refuses an untypeable grapheme', async () => {
  const service = await stub();
  const client = new ViewerClient(service.url);
  try {
    await client.acquire();
    const text = 'ქართული 🧑🏽‍💻 '.repeat(20);
    await client.type(text);
    const batches = posts(service.requests, 'input').map((request) => request.body.actions);
    assert.ok(batches.length > 1);
    assert.ok(batches.every((actions) => actions.length === 1 && actions[0].type === 'type'));
    assert.ok(batches.every(([action]) => Array.from(action.text).length <= 128));
    assert.equal(batches.map(([action]) => action.text).join(''), text);
    await assert.rejects(client.type('x' + '́'.repeat(300)), /too long to type/);
  } finally {
    client.stop();
    await service.close();
  }
});

test('clicks, drags, wheels and keys have the native viewer’s shapes', async () => {
  const service = await stub();
  const client = new ViewerClient(service.url);
  try {
    await client.acquire();
    await client.doubleClick(10, 20, { delay: 0 });
    await client.click(30, 40, { button: 'right', modifiers: ['shift'] });
    await client.drag({ x: 0, y: 0 }, { x: 10, y: 20 }, 2);
    await client.wheel(5, 6, 0, -3);
    await client.key('shift+left');
    const batches = posts(service.requests, 'input').map((request) => request.body.actions);
    const pointer = (phase, x, y, button, more) => ({
      type: 'pointer',
      phase,
      x,
      y,
      button,
      ...more,
    });
    assert.deepEqual(batches, [
      [{ type: 'move', x: 10, y: 20 }],
      [pointer('down', 10, 20, 'left', { count: 1, modifiers: [] })],
      [pointer('up', 10, 20, 'left', { modifiers: [] })],
      [pointer('down', 10, 20, 'left', { count: 2, modifiers: [] })],
      [pointer('up', 10, 20, 'left', { modifiers: [] })],
      [{ type: 'move', x: 30, y: 40 }],
      [pointer('down', 30, 40, 'right', { count: 1, modifiers: ['shift'] })],
      [pointer('up', 30, 40, 'right', { modifiers: ['shift'] })],
      [{ type: 'move', x: 0, y: 0 }],
      [pointer('down', 0, 0, 'left', { count: 1, modifiers: [] })],
      [pointer('move', 5, 10, 'left', { modifiers: [] })],
      [pointer('move', 10, 20, 'left', { modifiers: [] })],
      [pointer('up', 10, 20, 'left', { modifiers: [] })],
      [
        { type: 'move', x: 5, y: 6 },
        { type: 'wait', ms: 80 },
        { type: 'scroll', x: 5, y: 6, dx: 0, dy: -3 },
      ],
      [{ type: 'key', key: 'shift+left' }],
    ]);
  } finally {
    client.stop();
    await service.close();
  }
});

const busy = [503, { ok: false, error: 'Viewer is busy' }];
const active = {
  ok: true,
  controlActive: true,
  session: { inputService: { controlActive: true, heldInput: true } },
};

test('a failed renewal stops input and still sends the owner release', async () => {
  const service = await stub((route) => (route === 'control/renew' ? busy : null));
  const client = new ViewerClient(service.url, { frameEvery: 1000, renewEvery: 20 });
  try {
    await client.acquire();
    await until(
      () =>
        client.lease === null &&
        client.owner === null &&
        posts(service.requests, 'control/release').length > 0,
    );
    assert.equal(client.lease, null);
    assert.equal(client.owner, null);
    await assert.rejects(client.key('escape'), { status: 503 });
    assert.equal(posts(service.requests, 'control/renew').length, 1);
    assert.deepEqual(
      posts(service.requests, 'control/release').map((request) => request.body),
      [{ lease: 'lease-1' }],
    );
    assert.equal(posts(service.requests, 'input').length, 0);
  } finally {
    client.stop();
    await service.close();
  }
});

test('a lost mouse-up keeps the button held, ends control and is never replayed', async () => {
  let releases = 0;
  const service = await stub((route, body) => {
    if (route === 'input' && body.actions[0].phase === 'up') return 'drop';
    if (route === 'control/release' && ++releases === 1) return busy;
    if (route === 'status.json') return [200, active];
    return null;
  });
  const client = new ViewerClient(service.url, { frameEvery: 1000, renewEvery: 1000 });
  try {
    await client.acquire();
    await client.down(10, 20);
    assert.equal(client.held, 'left');
    await assert.rejects(client.up(10, 20), { uncertain: true });
    // The release at once failed, so nothing is confirmed yet.
    assert.equal(client.held, 'left');
    assert.equal(client.owner, 'lease-1');
    assert.equal(client.lease, null);
    assert.equal(client.cleanupError.status, 503);
    assert.equal(posts(service.requests, 'control/release').length, 1);
    await assert.rejects(client.up(10, 20), { uncertain: true });
    await client.release();
    assert.equal(client.held, null);
    assert.equal(client.owner, null);
    const ups = posts(service.requests, 'input').filter(
      (request) => request.body.actions[0].phase === 'up',
    );
    assert.equal(ups.length, 1);
    assert.equal(posts(service.requests, 'control/renew').length, 0);
  } finally {
    client.stop();
    await service.close();
  }
});

test('a failed release is reported and can be retried', async () => {
  let releases = 0;
  const service = await stub((route) => {
    if (route === 'control/release' && ++releases === 1) return 'drop';
    if (route === 'status.json') return [200, active];
    return null;
  });
  const client = new ViewerClient(service.url);
  try {
    await client.acquire();
    await assert.rejects(client.release(), { uncertain: true });
    assert.equal(client.owner, 'lease-1');
    await assert.rejects(client.acquire(), /Release the earlier control session first/);
    assert.deepEqual(await client.release(), { ok: true });
    assert.equal(client.owner, null);
    assert.equal(posts(service.requests, 'control/release').length, 2);
    assert.ok(posts(service.requests, 'control/release').every((r) => r.body.lease === 'lease-1'));
  } finally {
    client.stop();
    await service.close();
  }
});

test('a refused release counts once the service shows nothing held', async () => {
  const service = await stub((route) => {
    if (route === 'control/release')
      return [409, { ok: false, error: 'Control was lost', code: 'lease-lost' }];
    if (route === 'status.json')
      return [
        200,
        {
          ok: true,
          controlActive: false,
          session: { inputService: { controlActive: false, heldInput: false } },
        },
      ];
    return null;
  });
  const client = new ViewerClient(service.url);
  try {
    await client.acquire();
    assert.deepEqual(await client.release(), { ok: true, released: false, confirmedBy: 'status' });
    assert.equal(client.owner, null);
  } finally {
    client.stop();
    await service.close();
  }
});
