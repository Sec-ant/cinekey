#!/usr/bin/env bash
# Package the generated browser and Node bindings for npm.
set -euo pipefail

version=${1:?expected the package version}
commit=${2:?expected the source commit}
repo_dir=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_dir"

mkdir -p pkg dist
staging=$(mktemp -d "$repo_dir/pkg/.npm-XXXXXXXX")
trap 'rm -rf "$staging"' EXIT

for bindings in web nodejs; do
    mkdir -p "$staging/$bindings"
    for file in cinekey.js cinekey.d.ts cinekey_bg.wasm cinekey_bg.wasm.d.ts; do
        cp "pkg/$bindings/$file" "$staging/$bindings/$file"
    done
done
printf '%s\n' '{"type":"commonjs"}' > "$staging/nodejs/package.json"
jq --arg version "$version" --arg commit "$commit" \
    '. + {version: $version, gitHead: $commit}' \
    crates/cinekey-wasm/npm/package.json > "$staging/package.json"
cp README.md LICENSE "$staging/"
# Keep the previous tarball until every input has been copied and packing succeeds.
npm pack "$staging" --ignore-scripts --pack-destination "$staging"
mv "$staging/cinekey-$version.tgz" "dist/cinekey-$version.tgz"
