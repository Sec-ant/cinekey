set default-list

export CARGO_TARGET_DIR := justfile_directory() + "/target"
eval_manifest := "tools/eval/Cargo.toml"

# Check Rust formatting.
fmt:
    cargo fmt --all --check
    cargo fmt --manifest-path {{eval_manifest}} --all --check

# Format Rust sources.
fmt-fix:
    cargo fmt --all
    cargo fmt --manifest-path {{eval_manifest}} --all

# Run Clippy with warnings treated as errors.
clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo clippy --locked --manifest-path {{eval_manifest}} --all-targets -- -D warnings

# Run parser, CLI, evaluation, and documentation tests.
test:
    cargo test --locked --workspace --lib --bins
    cargo test --locked --doc
    cargo check --locked --benches
    cargo check --locked --no-default-features --lib
    cargo test --locked --manifest-path {{eval_manifest}}

# Build library documentation.
[env("RUSTDOCFLAGS", "-D warnings")]
doc:
    cargo doc --locked --workspace --no-deps

# Run the project's quality checks.
check: fmt clippy test doc

# Build the release CLI and its ck hard link.
build:
    cargo build --locked --release
    bash scripts/alias.sh install target/release/cinekey

# Build the distribution CLI and its ck hard link.
dist:
    cargo build --locked --profile dist
    bash scripts/alias.sh install target/dist/cinekey

# Install the Cargo-managed CLI and its ck hard link.
install:
    cargo install --locked --path . --profile dist
    cinekey_bin=$(command -v cinekey) && bash scripts/alias.sh install "$cinekey_bin"

# Build Wasm, JavaScript glue, and TypeScript declarations for web or nodejs.
wasm js_target="web":
    cargo build --locked --profile dist -p cinekey-wasm --target wasm32-unknown-unknown
    wasm-bindgen --target {{js_target}} --remove-name-section --remove-producers-section --out-name cinekey --out-dir pkg/{{js_target}} target/wasm32-unknown-unknown/dist/cinekey_wasm.wasm

# Build both Wasm bindings and package them for npm.
npm-pack: (wasm "web") (wasm "nodejs")
    version=$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "cinekey") | .version') && bash scripts/npm-package.sh "$version" "$(git rev-parse HEAD)"

# Check the Rust library and JavaScript adapter for the Wasm target.
wasm-check:
    cargo check --locked --no-default-features --lib --target wasm32-unknown-unknown
    cargo clippy --locked -p cinekey-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings

# Run the Rust Wasm bindings tests in Node.
[env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER", "wasm-bindgen-test-runner")]
wasm-test:
    cargo test --locked -p cinekey-wasm --lib --target wasm32-unknown-unknown

# Remove the ck hard link and the Cargo-managed CLI.
uninstall:
    if cinekey_bin=$(command -v cinekey 2>/dev/null); then bash scripts/alias.sh uninstall "$cinekey_bin"; fi
    cargo uninstall cinekey

# Run the representative release benchmark.
bench:
    cargo bench --locked --bench parse

# Build the independent evaluation tool.
eval-build:
    cargo build --locked --release --manifest-path {{eval_manifest}}

# Run the evaluation tool; use `just eval --help` to list its commands.
[positional-arguments]
eval +args:
    cargo run --quiet --locked --release --manifest-path {{eval_manifest}} -- "$@"

# Reclaim shared Rust build artifacts.
clean:
    cargo clean

# Publish the committed package version through the tag-triggered release workflow.
release:
    #!/usr/bin/env bash
    set -euo pipefail
    test "$(git branch --show-current)" = main
    test -z "$(git status --porcelain)"
    version=$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "cinekey") | .version')
    tag="v$version"
    if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
        test "$(git cat-file -t "refs/tags/$tag")" = tag
        test "$(git rev-parse "$tag^{commit}")" = "$(git rev-parse HEAD)"
    else
        git tag -a "$tag" -m "cinekey $version"
    fi
    git push --atomic origin HEAD "refs/tags/$tag"
