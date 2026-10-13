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

# Build the release CLI.
build:
    cargo build --locked --release

# Build the distribution CLI with full LTO and aborting panics.
dist:
    cargo build --locked --profile dist

# Install the Cargo-managed CLI.
install:
    cargo install --locked --path . --profile dist

# Build Wasm, JavaScript glue, and TypeScript declarations for web or nodejs.
wasm js_target="web":
    cargo build --locked --profile dist -p cinekey-wasm --target wasm32-unknown-unknown
    wasm-bindgen --target {{js_target}} --remove-name-section --remove-producers-section --out-name cinekey --out-dir pkg/{{js_target}} target/wasm32-unknown-unknown/dist/cinekey_wasm.wasm

# Check the Rust library and JavaScript adapter for the Wasm target.
wasm-check:
    cargo check --locked --no-default-features --lib --target wasm32-unknown-unknown
    cargo clippy --locked -p cinekey-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings

# Run the Rust Wasm bindings tests in Node.
[env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER", "wasm-bindgen-test-runner")]
wasm-test:
    cargo test --locked -p cinekey-wasm --lib --target wasm32-unknown-unknown

# Uninstall the Cargo-managed CLI.
uninstall:
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
