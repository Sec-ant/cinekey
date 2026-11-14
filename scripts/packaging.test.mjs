import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function directory(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cinekey-packaging-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

test('npm packaging excludes stale staging files and preserves previous archives on failure', t => {
  const root = directory(t);
  fs.mkdirSync(path.join(root, 'scripts'));
  fs.copyFileSync(path.join(repository, 'scripts/npm-package.sh'), path.join(root, 'scripts/npm-package.sh'));
  fs.mkdirSync(path.join(root, 'crates/cinekey-wasm/npm'), { recursive: true });
  fs.copyFileSync(path.join(repository, 'crates/cinekey-wasm/npm/package.json'), path.join(root, 'crates/cinekey-wasm/npm/package.json'));
  for (const file of ['README.md', 'LICENSE']) fs.writeFileSync(path.join(root, file), 'fixture\n');
  const required = ['cinekey.js', 'cinekey.d.ts', 'cinekey_bg.wasm', 'cinekey_bg.wasm.d.ts'];
  for (const target of ['web', 'nodejs']) {
    fs.mkdirSync(path.join(root, 'pkg', target), { recursive: true });
    for (const file of required) fs.writeFileSync(path.join(root, 'pkg', target, file), 'generated fixture\n');
  }
  fs.mkdirSync(path.join(root, 'pkg/npm/web'), { recursive: true });
  fs.writeFileSync(path.join(root, 'pkg/npm/web/obsolete.js'), 'stale\n');
  fs.mkdirSync(path.join(root, 'dist'));
  const archive = path.join(root, 'dist/cinekey-0.4.4.tgz');
  fs.writeFileSync(archive, 'previous tarball');
  const missing = path.join(root, 'pkg/nodejs/cinekey_bg.wasm');
  fs.unlinkSync(missing);
  const args = [path.join(root, 'scripts/npm-package.sh'), '0.4.4', 'commit'];
  assert.notEqual(spawnSync('bash', args, { encoding: 'utf8' }).status, 0);
  assert.equal(fs.readFileSync(archive, 'utf8'), 'previous tarball');
  fs.writeFileSync(missing, 'generated fixture\n');
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'npm'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
  assert.notEqual(spawnSync('bash', args, { env: { ...process.env, PATH: `${bin}:${process.env.PATH}` } }).status, 0);
  assert.equal(fs.readFileSync(archive, 'utf8'), 'previous tarball');
  execFileSync('bash', args, { stdio: 'pipe' });
  const entries = execFileSync('tar', ['-tzf', archive], { encoding: 'utf8' }).trim().split('\n').sort();
  assert.equal(entries.includes('package/web/obsolete.js'), false);
  for (const target of ['web', 'nodejs']) for (const file of required) assert.ok(entries.includes(`package/${target}/${file}`));
  assert.equal(fs.readdirSync(path.join(root, 'pkg')).some(name => name.startsWith('.npm-')), false);
});

test('Homebrew rendering validates checksums before replacing the formula', t => {
  const root = directory(t);
  fs.mkdirSync(path.join(root, 'scripts'));
  fs.copyFileSync(path.join(repository, 'scripts/homebrew-formula.sh'), path.join(root, 'scripts/homebrew-formula.sh'));
  fs.mkdirSync(path.join(root, 'homebrew-tap/Formula'), { recursive: true });
  const formula = path.join(root, 'homebrew-tap/Formula/cinekey.rb');
  fs.writeFileSync(formula, 'previous formula\n');
  const workflow = fs.readFileSync(path.join(repository, '.github/workflows/homebrew.yml'), 'utf8');
  const start = workflow.indexOf('          formula=$(mktemp');
  const end = workflow.indexOf('          if git -C', start);
  assert.ok(start > 0 && end > start);
  const script = `set -euo pipefail\nversion=0.4.4\n${workflow.slice(start, end)}`;
  const options = { cwd: root, env: { ...process.env, RUNNER_TEMP: root }, encoding: 'utf8' };
  assert.notEqual(spawnSync('bash', ['-c', script], options).status, 0);
  assert.equal(fs.readFileSync(formula, 'utf8'), 'previous formula\n');
  const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu'];
  fs.writeFileSync(path.join(root, 'SHA256SUMS'), targets.map(target => `${'a'.repeat(64)}  cinekey-0.4.4-${target}\n`).join(''));
  execFileSync('bash', ['-c', script], options);
  assert.match(fs.readFileSync(formula, 'utf8'), /class Cinekey < Formula/);
  assert.equal(fs.readdirSync(path.dirname(formula)).length, 1);
});
