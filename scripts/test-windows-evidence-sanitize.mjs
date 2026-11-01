import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { sanitizeEvidence } from './windows-evidence-sanitize.mjs';

const root = 'D:\\repos\\projectpane\\artifacts\\owned-fixture';
const home = 'C:\\Users\\private-fixture-account';
const repo = fileURLToPath(new URL('../', import.meta.url)).replace(/[\\/]$/, '');
const replacements = [[root, '<fixture>'], [home, '<home>'], [repo, '<repo>']];
test('redacts ordinary and slash paths without changing structured evidence', () => {
  const result = JSON.parse(sanitizeEvidence({ files: [root + '\\file.txt', home.replaceAll('\\', '/') + '/profile'], count: 2, saved: true }, replacements));
  assert.deepEqual(result, { files: ['<fixture>\\file.txt', '<home>/profile'], count: 2, saved: true });
});
test('redacts actual assertion messages containing inspected escaped paths', () => {
  let failure;
  try { assert.deepEqual({ files: [root + '\\source.txt', home + '\\profile'] }, { files: [] }); }
  catch (error) { failure = { message: String(error), stack: error.stack }; }
  assert(failure);
  const sanitized = sanitizeEvidence(failure, replacements);
  const result = JSON.parse(sanitized);
  assert(result.message.includes('<fixture>') && result.message.includes('<home>'));
  assert(!sanitized.includes('private-fixture-account') && !sanitized.includes('projectpane'));
});
