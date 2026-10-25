import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, writeFileSync, copyFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import plugin from '../agents/atlas/opencode/atlas.js';

test('opencode installs the bundled MCP and skill without replacing personal configuration', async () => {
  assert.equal(plugin.id, 'atlas');
  const hooks = await plugin.server();
  const config = {};
  await hooks.config(config);
  assert.equal(config.mcp.atlas.type, 'local');
  assert.ok(config.mcp.atlas.command[0].endsWith('/agents/atlas/scripts/atlas'));
  assert.equal(config.mcp.atlas.command[1], 'mcp');
  assert.ok(config.skills.paths[0].endsWith('/agents/atlas/skills/atlas'));
  assert.equal(config.permission.atlas_screen_frame, 'deny');
  assert.equal(config.permission.atlas_stop_agent, 'deny');
  await hooks.config(config);
  assert.equal(config.skills.paths.length, 1);
  const personal = { mcp: { atlas: { enabled: false } }, skills: { paths: ['/personal'] } };
  await hooks.config(personal);
  assert.deepEqual(personal.mcp.atlas, { enabled: false });
  assert.equal(personal.permission, undefined);
  assert.equal(personal.skills.paths[0], '/personal');
});

test('opencode caches a stable brief for each session, including concurrent requests, and clears deleted sessions', async () => {
  const root = mkdtempSync(join(tmpdir(), 'atlas-opencode-'));
  try {
    mkdirSync(join(root, 'opencode'));
    mkdirSync(join(root, 'scripts'));
    writeFileSync(join(root, 'package.json'), '{"type":"module"}');
    copyFileSync(
      new URL('../agents/atlas/opencode/atlas.js', import.meta.url),
      join(root, 'opencode/atlas.js'),
    );
    writeFileSync(
      join(root, 'scripts/status'),
      `#!/usr/bin/env node
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
const file=fileURLToPath(new URL('../calls',import.meta.url));
fs.appendFileSync(file,'x');
console.log('Atlas: '+fs.readFileSync(file,'utf8').length);
`,
      { mode: 0o755 },
    );
    const { default: isolated } = await import(pathToFileURL(join(root, 'opencode/atlas.js')));
    const hooks = await isolated.server();
    const transform = hooks['experimental.chat.system.transform'];
    const first = { system: ['Personal instruction'] };
    const second = { system: [] };
    await Promise.all([
      transform({ sessionID: 'one' }, first),
      transform({ sessionID: 'one' }, second),
    ]);
    assert.deepEqual(first.system, ['Personal instruction', 'Atlas: 1']);
    assert.deepEqual(second.system, ['Atlas: 1']);
    await transform({ sessionID: 'one' }, first);
    assert.equal(first.system.length, 2);
    await transform({ sessionID: 'two' }, { system: [] });
    assert.equal(readFileSync(join(root, 'calls'), 'utf8').length, 2);
    await hooks.event({ event: { type: 'session.deleted', properties: { info: { id: 'one' } } } });
    const fresh = { system: [] };
    await transform({ sessionID: 'one' }, fresh);
    assert.deepEqual(fresh.system, ['Atlas: 3']);
    writeFileSync(join(root, 'scripts/status'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
    const failed = { system: ['Keep me'] };
    await transform({ sessionID: 'failed' }, failed);
    assert.deepEqual(failed.system, ['Keep me']);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('opencode leaves non-session requests alone and refuses only shell actor overrides', async () => {
  const hooks = await plugin.server();
  const output = { system: ['Existing'] };
  await hooks['experimental.chat.system.transform']({}, output);
  assert.deepEqual(output.system, ['Existing']);
  await hooks['tool.execute.before']({ tool: 'bash' }, { args: { command: 'atlas doctor' } });
  await hooks['tool.execute.before']({ tool: 'write' }, { args: { command: 'ATLAS_ACTOR' } });
  await assert.rejects(
    hooks['tool.execute.before'](
      { tool: 'bash' },
      { args: { command: 'ATLAS_ACTOR=viewer atlas resume' } },
    ),
    /Only the person/,
  );
});
