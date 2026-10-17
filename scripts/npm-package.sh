#!/usr/bin/env bash
# Package the generated browser and Node bindings for npm.
set -euo pipefail

version=${1:?expected the package version}
commit=${2:?expected the source commit}
repo_dir=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_dir"

for bindings in web nodejs; do
    mkdir -p "pkg/npm/$bindings"
    for file in cinekey.js cinekey.d.ts cinekey_bg.wasm cinekey_bg.wasm.d.ts; do
        cp "pkg/$bindings/$file" "pkg/npm/$bindings/$file"
    done
done
printf '%s\n' '{"type":"commonjs"}' > pkg/npm/nodejs/package.json
jq --arg version "$version" --arg commit "$commit" \
    '. + {version: $version, gitHead: $commit}' \
    crates/cinekey-wasm/npm/package.json > pkg/npm/package.json
cp README.md LICENSE pkg/npm/
mkdir -p dist
npm pack ./pkg/npm --ignore-scripts --pack-destination dist
