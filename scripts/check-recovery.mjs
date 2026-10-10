// Real native ownership/cancellation/crash tests. Only the prepared fixture is used.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';
import crypto from 'node:crypto';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';

const output = path.resolve(process.argv[2] || 'runs');
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
// The CLI resolves the TOML configuration (UNDERSTUDY_CONFIG or the default path).
const resolved = spawnSync('target/debug/understudy', ['--json', 'config'], { encoding: 'utf8' });
if (resolved.status !== 0) throw new Error(resolved.stderr || resolved.stdout);
const { config } = JSON.parse(resolved.stdout);
// Never drive a fixture prepared against a different helper or socket.
if (setup.config && JSON.stringify(setup.config) !== JSON.stringify(config))
  throw new Error('The resolved configuration differs from the one the fixture was prepared with');
const socketPath = config.input_socket;
const readState = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = { startedAt: new Date().toISOString(), actualHiddenDesktop: true, checks: {} };
function call(request, socket = socketPath) {
  return new Promise((resolve, reject) => {
    const stream = net.createConnection(socket);
    let bytes = '';
    stream.setTimeout(7000, () => stream.destroy(new Error('native request timed out')));
    stream.once('connect', () => stream.write(JSON.stringify(request) + '\n'));
    stream.on('data', (chunk) => {
      bytes += chunk;
      if (bytes.includes('\n')) {
        resolve(JSON.parse(bytes.slice(0, bytes.indexOf('\n'))));
        stream.destroy();
      }
    });
    stream.once('error', reject);
    stream.once('close', () => {
      if (!bytes.includes('\n')) reject(new Error('no reply'));
    });
  });
}
async function cli(args) {
  return new Promise((resolve) => {
    const child = spawn('./understudy', ['--json', ...args]);
    let out = '',
      err = '';
    child.stdout.on('data', (b) => (out += b));
    child.stderr.on('data', (b) => (err += b));
    child.on('close', (code) =>
      resolve({
        code,
        out,
        err,
        value: (() => {
          try {
            return JSON.parse(out || err);
          } catch {
            return null;
          }
        })(),
      }),
    );
  });
}
async function until(predicate) {
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await wait(10);
  }
  throw new Error('Native application state did not settle: ' + JSON.stringify(readState()));
}
let owner, epoch;
async function acquire() {
  const hello = await call({ op: 'hello' });
  assert.equal(hello.ready, true);
  epoch = hello.enforcedEpoch;
  owner = crypto.randomBytes(24).toString('hex');
  assert.equal((await call({ op: 'acquire', owner, epoch })).ok, true);
}
function pointer(phase, modifiers = []) {
  const state = readState();
  return {
    type: 'pointer',
    phase,
    x: state.windowX + 300,
    y: 1440 - state.windowY - 200,
    modifiers,
  };
}
async function hold(modifiers = []) {
  assert.equal(
    (await call({ op: 'input', owner, epoch, actions: [pointer('down', modifiers)] })).ok,
    true,
  );
  await until(() => readState().mouseHeld);
}
async function release() {
  assert.equal((await call({ op: 'release', owner })).ok, true);
  owner = null;
  await until(() => !readState().mouseHeld);
}
function disconnect(request) {
  return new Promise((resolve) => {
    const stream = net.createConnection(socketPath);
    stream.on('connect', () => {
      stream.write(JSON.stringify(request) + '\n');
      setTimeout(() => {
        stream.destroy();
        resolve();
      }, 60);
    });
    stream.on('error', resolve);
  });
}
try {
  await acquire();
  await hold();
  const action = call({
    op: 'input',
    owner,
    epoch,
    actions: [
      ...Array.from({ length: 6 }, () => ({ type: 'wait', ms: 250 })),
      { type: 'click', x: 10, y: 10 },
    ],
  });
  await wait(100);
  const releaseAt = performance.now();
  await release();
  const reply = await action;
  assert.equal(reply.ok, false);
  assert.equal(reply.code, 'lease-lost');
  results.checks.cancelDuringBatch = {
    passed: true,
    releaseToNativeUpMs: performance.now() - releaseAt,
    reply,
  };

  await acquire();
  await hold();
  await disconnect({ op: 'input' });
  assert.equal((await call({ op: 'hello' })).controlActive, true);
  assert.equal(readState().mouseHeld, true);
  await release();
  results.checks.malformedDisconnectCannotCancelOwner = { passed: true };

  await acquire();
  await hold();
  await disconnect({
    op: 'input',
    owner,
    epoch,
    actions: Array.from({ length: 6 }, () => ({ type: 'wait', ms: 250 })),
  });
  await until(() => !readState().mouseHeld);
  assert.equal((await call({ op: 'hello' })).controlActive, false);
  owner = null;
  results.checks.disconnectCleansHeldButton = { passed: true };

  await acquire();
  await hold();
  await wait(5650);
  const expiryAt = performance.now();
  const expired = await call({
    op: 'input',
    owner,
    epoch,
    actions: Array.from({ length: 8 }, () => ({ type: 'wait', ms: 250 })),
  });
  await until(() => !readState().mouseHeld);
  assert.equal(expired.ok, false);
  assert.equal(expired.code, 'lease-lost');
  assert.ok(performance.now() - expiryAt < 1000);
  results.checks.expiryInterruptsBatch = {
    passed: true,
    elapsedMs: performance.now() - expiryAt,
    reply: expired,
  };
  owner = null;

  await acquire();
  const wrong = await call({
    op: 'input',
    owner,
    epoch: { ...epoch, displayId: epoch.displayId + 1 },
    actions: [pointer('down')],
  });
  assert.equal(wrong.code, 'session-changed');
  assert.equal(readState().mouseHeld, false);
  await release();
  results.checks.wrongDisplayRefused = { passed: true };

  await acquire();
  await hold(['shift']);
  const pid = epoch.processId;
  assert.ok((readState().modifierFlags & (1 << 17)) !== 0, 'AppKit did not receive Shift');
  const killed = await cli(['run', '--', '/bin/kill', '-KILL', String(pid)]);
  assert.equal(killed.code, 0, killed.err);
  const beforeRestart = readState();
  const startAt = performance.now();
  const started = await cli(['start-input']);
  assert.equal(started.code, 0, started.err);
  await until(() => !readState().mouseHeld && readState().modifierFlags === 0);
  const recovered = await call({ op: 'hello' });
  assert.notEqual(recovered.pid, pid);
  assert.equal(recovered.heldInput, false);
  results.checks.sigkillHeldButtonRecovery = {
    passed: true,
    beforeRestart,
    afterRestart: readState(),
    oldPID: pid,
    newPID: recovered.pid,
    elapsedMs: performance.now() - startAt,
  };
  owner = null;

  results.passed = true;
} catch (error) {
  results.passed = false;
  results.error = error.message;
} finally {
  if (owner) await call({ op: 'release', owner }).catch(() => {});
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'native-recovery-verification.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
  if (!results.passed) process.exitCode = 1;
}
