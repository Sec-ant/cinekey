# Changelog
## 0.4.4 (2026-10-17)

### Fixes

- Distribute one standalone `cinekey` executable per platform (`cinekey.exe` on Windows).

## 0.4.3 (2026-10-17)

### Features

#### Extract video bit depth

Expose `video_bit_depth` in Rust, CLI, and Wasm output for explicit 8-, 10-, and
12-bit release markers, including `Hi10P`. Matching markers share one numeric
value; conflicting markers leave the field absent.

### Fixes

#### Publish when the Release PR merges

Merging the Release PR starts an observable Actions workflow that prepares the
version tag and launches publishing for the crate, npm package, and release assets.

## 0.4.2 (2026-10-17)

### Fixes

#### Automate release preparation

Prepare version and changelog updates from change files. Merging the Release PR
publishes the Rust crate, Wasm npm package, and CLI archives together.
