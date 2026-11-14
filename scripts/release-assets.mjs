import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const runGh = args => execFileSync('gh', args, { encoding: 'utf8' });
const checksum = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');

function checksums(file) {
  const entries = new Map();
  for (const line of fs.readFileSync(file, 'utf8').trim().split('\n')) {
    const match = /^([a-f0-9]{64})  (cinekey-[A-Za-z0-9._+-]+)$/.exec(line);
    if (!match || entries.has(match[2])) throw new Error('Invalid or duplicate release checksum');
    entries.set(match[2], match[1]);
  }
  return entries;
}

export function uploadAssets(tag, directory, gh = runGh) {
  const files = fs.readdirSync(directory).filter(name => /^cinekey-[A-Za-z0-9._+-]+$/.test(name)).sort();
  if (!files.length) throw new Error('No release assets');
  const remote = new Set(JSON.parse(gh(['release', 'view', tag, '--json', 'assets'])).assets.map(asset => asset.name));
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'cinekey-release-'));
  const manifest = 'SHA256SUMS';
  try {
    const download = name => {
      gh(['release', 'download', tag, '--pattern', name, '--dir', temporary]);
      return path.join(temporary, name);
    };
    const expected = remote.has(manifest) ? checksums(download(manifest)) : null;
    const pending = [];
    const actual = [];
    // Validate the entire plan before uploading any missing file.
    for (const name of files) {
      const local = path.join(directory, name);
      const published = remote.has(name) ? download(name) : null;
      const digest = checksum(published ?? local);
      if (expected && expected.get(name) !== digest) {
        throw new Error(`Release asset ${name} does not match the published SHA256SUMS`);
      }
      if (published) {
        // Keep release downloads stable across retries.
        fs.copyFileSync(published, local);
      } else {
        pending.push(local);
      }
      actual.push(`${digest}  ${name}\n`);
    }
    fs.writeFileSync(path.join(directory, manifest), actual.join(''));
    for (const file of pending) gh(['release', 'upload', tag, file]);
    // Publish the checksum manifest after the assets.
    if (!expected) gh(['release', 'upload', tag, path.join(directory, manifest)]);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [tag, directory] = process.argv.slice(2);
  if (!tag || !directory) throw new Error('Expected <tag> <asset directory>');
  uploadAssets(tag, directory);
}
