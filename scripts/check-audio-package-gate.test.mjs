#!/usr/bin/env node
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const result = spawnSync(process.execPath, [resolve(here, 'check-audio-package-gate.mjs')], {
  encoding: 'utf8',
});
assert.notEqual(result.status, 0, 'the repository must remain blocked without licensed driver artifacts');
assert.match(result.stderr, /license gate failed/);
console.log('Audio package gate test passed: missing licensed artifacts block packaging.');
