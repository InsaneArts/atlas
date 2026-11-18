// release.sh refuses to make a release build when any local-test variable is
// present, empty or not: each such variable changes the released code.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import test from 'node:test';

const script = fs.readFileSync(new URL('../scripts/release.sh', import.meta.url), 'utf8');

// The guard loop, lifted from the script and run on its own.
const guard = /for variable in [\s\S]*?\ndone/.exec(script);
assert.ok(guard, 'release.sh has the test-variable guard loop');
const fail = 'fail() { printf "%s\\n" "$*" >&2; exit 1; }\n';

function guarded(env) {
  return spawnSync('/bin/sh', ['-c', fail + guard[0]], { env: { ...env } }).status;
}

const variables = [
  'ATLAS_LOCAL_TEAM',
  'ATLAS_TEST_HOLD_RELEASE',
  'ATLAS_TEST_HAND_RUN',
  'ATLAS_TEST_FAKE_GRANT',
  'ATLAS_TEST_DROP_REPLY',
  'ATLAS_TEST_WORKER_PAUSE',
];

test('a release with no test variable set is allowed', () => {
  assert.equal(guarded({ PATH: process.env.PATH }), 0);
});

for (const variable of variables) {
  test(`a release with ${variable} present is refused, empty or set`, () => {
    for (const value of ['', 'anything']) {
      assert.equal(
        guarded({ PATH: process.env.PATH, [variable]: value }),
        1,
        `${variable}=${JSON.stringify(value)}`,
      );
    }
  });
}
