import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { uploadAssets } from './release-assets.mjs';

const name = 'cinekey-0.4.4-wasm-web.tar.gz';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

function setup(t, remote = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cinekey-assets-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  fs.writeFileSync(path.join(directory, name), 'rebuilt bytes');
  const calls = [];
  let failUpload = false;
  const gh = args => {
    calls.push(args);
    if (args[1] === 'view') return JSON.stringify({ assets: Object.keys(remote).map(name => ({ name })) });
    if (args[1] === 'download') {
      const asset = args[args.indexOf('--pattern') + 1];
      fs.writeFileSync(path.join(args[args.indexOf('--dir') + 1], asset), remote[asset]);
    }
    if (args[1] === 'upload') {
      if (failUpload) { failUpload = false; throw new Error('upload interrupted'); }
      const asset = path.basename(args[3]);
      assert.equal(remote[asset], undefined, 'release assets retain their original bytes');
      remote[asset] = fs.readFileSync(args[3], 'utf8');
    }
    return '';
  };
  return { directory, gh, remote, calls, interrupt() { failUpload = true; } };
}

test('completed releases preserve original bytes across rebuilds and reruns', t => {
  const previous = 'published bytes';
  const data = setup(t, { [name]: previous, SHA256SUMS: `${hash(previous)}  ${name}\n` });
  uploadAssets('v0.4.4', data.directory, data.gh);
  assert.equal(fs.readFileSync(path.join(data.directory, name), 'utf8'), previous);
  assert.equal(data.calls.filter(call => call[1] === 'upload').length, 0);
});

test('missing assets are completed using the original checksum manifest', t => {
  const data = setup(t, { SHA256SUMS: `${hash('rebuilt bytes')}  ${name}\n` });
  uploadAssets('v0.4.4', data.directory, data.gh);
  assert.equal(data.remote[name], 'rebuilt bytes');
  assert.equal(data.calls.filter(call => call[1] === 'upload').length, 1);
});

test('checksum conflicts stop before any upload', t => {
  const data = setup(t, { SHA256SUMS: `${hash('different build')}  ${name}\n` });
  assert.throws(() => uploadAssets('v0.4.4', data.directory, data.gh), /does not match/);
  assert.equal(data.calls.filter(call => call[1] === 'upload').length, 0);
});

test('interrupted uploads resume and publish the manifest last', t => {
  const data = setup(t);
  data.interrupt();
  assert.throws(() => uploadAssets('v0.4.4', data.directory, data.gh), /interrupted/);
  uploadAssets('v0.4.4', data.directory, data.gh);
  assert.equal(data.remote.SHA256SUMS, `${hash(data.remote[name])}  ${name}\n`);
  assert.equal(path.basename(data.calls.at(-1)[3]), 'SHA256SUMS');
  fs.writeFileSync(path.join(data.directory, name), 'another rebuild');
  uploadAssets('v0.4.4', data.directory, data.gh);
  assert.equal(data.remote[name], 'rebuilt bytes');
});

test('partially uploaded releases retain earlier assets when the remaining upload is retried', t => {
  const data = setup(t);
  const earlier = 'cinekey-0.4.4-cli';
  fs.writeFileSync(path.join(data.directory, earlier), 'first build');
  let uploads = 0;
  const interrupted = args => {
    if (args[1] === 'upload' && ++uploads === 2) throw new Error('second upload interrupted');
    return data.gh(args);
  };
  assert.throws(() => uploadAssets('v0.4.4', data.directory, interrupted), /interrupted/);
  assert.equal(data.remote[earlier], 'first build');
  assert.equal(data.remote.SHA256SUMS, undefined);
  fs.writeFileSync(path.join(data.directory, earlier), 'second build');
  uploadAssets('v0.4.4', data.directory, data.gh);
  assert.equal(data.remote[earlier], 'first build');
  assert.ok(data.remote.SHA256SUMS.includes(`${hash('first build')}  ${earlier}`));
});
