import assert from 'node:assert/strict';
import test from 'node:test';
import { publish, promote } from './npm-publish.mjs';

function registry(published, tags) {
  const calls = [];
  return {
    manageTags: true,
    calls,
    npm: args => calls.push(args),
    fetchRegistry: async url => ({
      status: url.endsWith('/cinekey') || published ? 200 : 404,
      json: async () => url.endsWith('/cinekey') ? { 'dist-tags': tags } : published,
    }),
  };
}

test('uploading a missing old version leaves latest unchanged', async () => {
  const dependencies = registry(null, { latest: '0.4.4' });
  await publish('0.4.3', 'commit', 'package.tgz', dependencies);
  assert.deepEqual(dependencies.calls, [['publish', 'package.tgz', '--tag', 'release-0.4.3']]);
});

test('publish-only trusted configurations publish newer versions and retain channel ordering', async () => {
  for (const [version, latest, tag] of [['0.4.5', '0.4.4', 'latest'], ['0.4.3', '0.4.4', 'release-0.4.3']]) {
    const dependencies = registry(null, { latest });
    dependencies.manageTags = false;
    await publish(version, 'commit', 'package.tgz', dependencies);
    await promote(version, 'commit', dependencies);
    assert.deepEqual(dependencies.calls, [['publish', 'package.tgz', '--tag', tag]]);
  }
  const dependencies = registry({ gitHead: 'commit' }, { latest: '0.4.2' });
  dependencies.manageTags = false;
  await publish('0.4.4', 'commit', 'package.tgz', dependencies);
  await promote('0.4.4', 'commit', dependencies);
  assert.deepEqual(dependencies.calls, []);
});

test('existing versions validate identity and are not uploaded again', async () => {
  const dependencies = registry({ gitHead: 'commit' }, {});
  await publish('0.4.4', 'commit', 'package.tgz', dependencies);
  assert.deepEqual(dependencies.calls, []);
  await assert.rejects(publish('0.4.4', 'other', 'package.tgz', dependencies), /different source commit/);
});

test('promotion repairs incomplete channels, skips older versions, and compares SemVer', async () => {
  for (const [version, current, promoteTo] of [
    ['0.4.4', '0.4.2', 'latest'],
    ['0.4.3', '0.4.4', null],
    ['0.4.4', '0.4.4', null],
    ['0.4.10', '0.4.9', 'latest'],
    ['0.5.0-rc.10', '0.5.0-rc.2', 'next'],
    ['0.5.0-rc.2', '0.5.0-rc.10', null],
    ['0.4.4', undefined, 'latest'],
  ]) {
    const channel = version.includes('-') ? 'next' : 'latest';
    const tags = { [`release-${version}`]: version };
    if (current !== undefined) tags[channel] = current;
    const dependencies = registry({ gitHead: 'commit' }, tags);
    await promote(version, 'commit', dependencies);
    assert.deepEqual(dependencies.calls, [
      ...(promoteTo ? [['dist-tag', 'add', `cinekey@${version}`, promoteTo]] : []),
      ['dist-tag', 'rm', 'cinekey', `release-${version}`],
    ]);
  }
});

test('registry failures and failed promotion leave recovery state intact', async () => {
  for (const status of [401, 429, 503]) {
    const dependencies = registry(null, {});
    dependencies.fetchRegistry = async () => ({ status });
    await assert.rejects(publish('0.4.4', 'commit', 'package.tgz', dependencies), /HTTP/);
    assert.deepEqual(dependencies.calls, []);
  }
  const dependencies = registry({ gitHead: 'commit' }, { latest: '0.4.2', 'release-0.4.4': '0.4.4' });
  dependencies.npm = args => { dependencies.calls.push(args); throw new Error('upload interrupted'); };
  await assert.rejects(promote('0.4.4', 'commit', dependencies), /interrupted/);
  assert.equal(dependencies.calls.length, 1);
  const invalid = registry(null, {});
  invalid.fetchRegistry = async () => ({ status: 200, json: async () => null });
  await assert.rejects(publish('0.4.4', 'commit', 'package.tgz', invalid), /Invalid npm registry/);
  assert.deepEqual(invalid.calls, []);
});
