// install.sh replaces /Applications/Atlas.app only when it is Atlas. These
// tests run its replaceable check against app bundles in a temporary folder.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

const script = fs.readFileSync(new URL('../install.sh', import.meta.url), 'utf8');
const macOS = process.platform === 'darwin';

function definition(pattern) {
  const match = pattern.exec(script);
  assert.ok(match, `install.sh has no ${pattern}`);
  return match[0];
}

const check = [
  definition(/^IDENTIFIER=.*$/m),
  definition(/^replaceable\(\) \{\n[\s\S]*?\n\}$/m),
].join('\n');

function replaceable(app) {
  const result = spawnSync('/bin/sh', ['-c', `${check}\nreplaceable "$1"`, 'sh', app]);
  assert.equal(result.error, undefined);
  return result.status === 0;
}

function bundle(folder, identifier) {
  const app = path.join(folder, 'Atlas.app');
  fs.mkdirSync(path.join(app, 'Contents', 'MacOS'), { recursive: true });
  fs.writeFileSync(path.join(app, 'Contents', 'MacOS', 'atlas'), '');
  if (identifier !== undefined) {
    fs.writeFileSync(
      path.join(app, 'Contents', 'Info.plist'),
      [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">',
        '<plist version="1.0">',
        `<dict><key>CFBundleIdentifier</key><string>${identifier}</string></dict>`,
        '</plist>',
      ].join('\n'),
    );
  }
  return app;
}

function withFolder(body) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-install-'));
  try {
    body(folder);
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
}

test('install.sh installs when no app is there', () => {
  withFolder((folder) => {
    assert.ok(replaceable(path.join(folder, 'Atlas.app')));
  });
});

test('install.sh replaces an app with the Atlas bundle id', { skip: !macOS }, () => {
  withFolder((folder) => {
    assert.ok(replaceable(bundle(folder, 'com.insanearts.atlas')));
  });
});

test('install.sh leaves another app named Atlas untouched', { skip: !macOS }, () => {
  withFolder((folder) => {
    assert.equal(replaceable(bundle(folder, 'com.openai.atlas')), false);
  });
});

test('install.sh leaves an app without a bundle id untouched', { skip: !macOS }, () => {
  withFolder((folder) => {
    assert.equal(replaceable(bundle(folder, undefined)), false);
  });
});

test('install.sh checks the app before it updates or downloads anything', () => {
  const guard = script.indexOf('replaceable "$dest" || fail');
  assert.ok(guard > 0, 'install.sh does not check the existing app');
  for (const step of ['exec "$installed"', 'mktemp -d', 'mv "$dest"']) {
    assert.ok(guard < script.indexOf(step), `the check comes after ${step}`);
  }
});
