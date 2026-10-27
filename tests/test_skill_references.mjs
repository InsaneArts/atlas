import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const skillFolder = fileURLToPath(new URL('../agents/atlas/skills/atlas/', import.meta.url));

function linkedReferences() {
  const skill = fs.readFileSync(path.join(skillFolder, 'SKILL.md'), 'utf8');
  return [...skill.matchAll(/\]\((references\/[^)]+)\)/g)].map((match) => match[1]);
}

test('the skill links to its references', () => {
  assert.ok(linkedReferences().length > 0);
});

test('every reference the skill links to exists in its folder', () => {
  for (const link of linkedReferences()) {
    const file = path.join(skillFolder, link);
    assert.ok(fs.statSync(file, { throwIfNoEntry: false })?.isFile(), `${link} is missing`);
  }
});
