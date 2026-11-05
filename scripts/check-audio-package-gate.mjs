#!/usr/bin/env node
import { existsSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, '..');
const manifestPath = resolve(root, 'src-tauri/resources/audio/manifest.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const errors = [];

for (const platform of ['windows', 'macos']) {
  const entry = manifest.platforms?.[platform];
  if (!entry?.package || !entry.sha256 || !entry.license_file) {
    errors.push(`${platform}: package, sha256, and license_file are required`);
    continue;
  }
  const packagePath = resolve(root, 'src-tauri/resources/audio', entry.package);
  const licensePath = resolve(root, 'src-tauri/resources/audio', entry.license_file);
  if (!existsSync(packagePath)) errors.push(`${platform}: missing package ${entry.package}`);
  if (!existsSync(licensePath)) errors.push(`${platform}: missing license ${entry.license_file}`);
  if (existsSync(packagePath)) {
    const actual = createHash('sha256').update(readFileSync(packagePath)).digest('hex');
    if (actual.toLowerCase() !== entry.sha256.toLowerCase()) errors.push(`${platform}: sha256 mismatch for ${entry.package}`);
  }
}

if (errors.length) {
  console.error('Audio package license gate failed:');
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}
console.log('Audio package license gate passed.');
