import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import extension from '../agents/atlas/pi/atlas.ts';

function host(exec = async () => ({ code: 0, stdout: 'Atlas: ready.\n' })) {
  const hooks = new Map();
  const servers = new Map();
  const calls = [];
  extension({
    on: (name, handler) => hooks.set(name, handler),
    registerMcpServer: (name, config) => servers.set(name, config),
    exec: (...args) => {
      calls.push(args);
      return exec(...args);
    },
  });
  return { hooks, servers, calls };
}

test('Pi exposes Atlas tools directly and loads only the Atlas skill', () => {
  const { servers } = host();
  const server = servers.get('atlas');
  assert.equal(server.exposure, 'hidden');
  assert.deepEqual(server.toolExposure, { atlas_status: 'direct', show_screen: 'direct' });
  assert.ok(server.command.endsWith('/agents/atlas/scripts/atlas'));
  assert.deepEqual(server.args, ['mcp']);
  const manifest = JSON.parse(
    readFileSync(new URL('../agents/atlas/package.json', import.meta.url)),
  );
  assert.deepEqual(manifest.pi.skills, ['./skills/atlas']);
  assert.deepEqual(manifest.pi.extensions, ['./pi/atlas.ts']);
});

test('the brief is shared by concurrent turns and refreshed for a new session', async () => {
  const { hooks, calls } = host();
  hooks.get('session_start')();
  const events = [0, 1].map(() => ({ systemPromptOptions: { sections: { other: 'Keep me' } } }));
  await Promise.all(events.map((event) => hooks.get('before_agent_start')(event)));
  assert.equal(calls.length, 1);
  for (const event of events) {
    assert.deepEqual(event.systemPromptOptions.sections, {
      other: 'Keep me',
      atlas: 'Atlas: ready.',
    });
  }
  hooks.get('session_start')();
  await hooks.get('before_agent_start')(events[0]);
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0].slice(1), [[], { timeout: 5000 }]);
});

test('a failed or timed out brief leaves the host prompt intact', async () => {
  for (const exec of [
    async () => ({ code: 1, stdout: 'Failure' }),
    async () => ({ code: 0, killed: true, stdout: 'Partial' }),
    async () => {
      throw new Error('Unavailable');
    },
  ]) {
    const { hooks } = host(exec);
    const event = { systemPromptOptions: { sections: { other: 'Keep me' } } };
    await hooks.get('before_agent_start')(event);
    assert.deepEqual(event.systemPromptOptions.sections, { other: 'Keep me' });
  }
});

test('Pi refuses shell actor overrides while allowing ordinary helper commands and file edits', () => {
  const guard = host().hooks.get('tool_call');
  for (const toolName of ['bash', 'powershell']) {
    assert.equal(guard({ toolName, input: { command: 'atlas doctor' } }), undefined);
    const result = guard({ toolName, input: { command: 'ATLAS_ACTOR=viewer atlas resume' } });
    assert.equal(result.block, true);
    assert.match(result.reason, /Only the person/);
  }
  assert.equal(guard({ toolName: 'write', input: { content: 'ATLAS_ACTOR' } }), undefined);
});
