import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import semver from 'semver';

const registry = 'https://registry.npmjs.org/cinekey';
const runNpm = args => execFileSync('npm', args, { stdio: 'inherit' });

async function read(url, fetchRegistry) {
  let response;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      response = await fetchRegistry(url, { signal: AbortSignal.timeout(30_000) });
    } catch (error) {
      if (attempt === 2) throw error;
    }
    if (response && response.status !== 429 && response.status < 500) break;
    if (attempt < 2) await new Promise(resolve => setTimeout(resolve, 100 * (attempt + 1)));
  }
  if (!response) throw new Error('npm registry request failed');
  if (response.status === 404) return null;
  if (response.status !== 200) throw new Error(`npm registry returned HTTP ${response.status}`);
  const body = await response.json();
  if (!body || typeof body !== 'object' || Array.isArray(body)) throw new Error('Invalid npm registry response');
  return body;
}

function validate(version) {
  if (!semver.valid(version)) throw new Error(`Invalid release version: ${version}`);
}

function channel(version) {
  return semver.prerelease(version) ? 'next' : 'latest';
}

function tagsOf(metadata) {
  const tags = metadata?.['dist-tags'];
  if (!tags || typeof tags !== 'object' || Array.isArray(tags)) throw new Error('Missing npm dist-tags');
  return tags;
}

function advances(version, tags, target) {
  const current = tags[target];
  if (current !== undefined && !semver.valid(current)) throw new Error(`Invalid ${target} version: ${current}`);
  return current === undefined || semver.gt(version, current);
}

async function identity(version, commit, fetchRegistry) {
  const published = await read(`${registry}/${encodeURIComponent(version)}`, fetchRegistry);
  if (published && published.gitHead !== commit) {
    throw new Error(`npm version ${version} belongs to a different source commit`);
  }
  return published;
}

export async function publish(version, commit, tarball, dependencies = {}) {
  validate(version);
  const fetchRegistry = dependencies.fetchRegistry ?? fetch;
  const npm = dependencies.npm ?? runNpm;
  if (!await identity(version, commit, fetchRegistry)) {
    // Version-specific tags keep uploads separate from channel promotion.
    let tag = `release-${version}`;
    if (!dependencies.manageTags) {
      const metadata = await read(registry, fetchRegistry);
      const tags = metadata === null ? {} : tagsOf(metadata);
      const target = channel(version);
      if (advances(version, tags, target)) tag = target;
    }
    npm(['publish', tarball, '--tag', tag]);
  }
}

export async function promote(version, commit, dependencies = {}) {
  validate(version);
  // Channel management follows the trusted publisher's configured permissions.
  if (!dependencies.manageTags) return;
  const fetchRegistry = dependencies.fetchRegistry ?? fetch;
  const npm = dependencies.npm ?? runNpm;
  if (!await identity(version, commit, fetchRegistry)) throw new Error(`npm version ${version} is not published`);
  const metadata = await read(registry, fetchRegistry);
  const tags = tagsOf(metadata);
  const target = channel(version);
  if (advances(version, tags, target)) {
    npm(['dist-tag', 'add', `cinekey@${version}`, target]);
  }
  // Retain the staging tag until channel promotion completes.
  if (tags[`release-${version}`] === version) {
    npm(['dist-tag', 'rm', 'cinekey', `release-${version}`]);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [operation, version, commit, tarball] = process.argv.slice(2);
  if (!version || !commit || !['publish', 'promote'].includes(operation) || (operation === 'publish' && !tarball)) {
    throw new Error('Expected publish <version> <commit> <tarball> or promote <version> <commit>');
  }
  const dependencies = { manageTags: process.env.NPM_DIST_TAG_ENABLED === 'true' };
  await (operation === 'publish' ? publish(version, commit, tarball, dependencies) : promote(version, commit, dependencies));
}
