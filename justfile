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
    cargo clippy --locked --all-targets -- -D warnings
    cargo clippy --locked --manifest-path {{eval_manifest}} --all-targets -- -D warnings

# Run parser, CLI, evaluation, and documentation tests.
test:
    cargo test --locked --lib --bins
    cargo test --locked --doc
    cargo check --locked --benches
    cargo test --locked --manifest-path {{eval_manifest}}

# Build library documentation.
[env("RUSTDOCFLAGS", "-D warnings")]
doc:
    cargo doc --locked --no-deps

# Run the project's quality checks.
check: fmt clippy test doc

# Build the release CLI.
build:
    cargo build --locked --release

# Install the Cargo-managed CLI.
install:
    cargo install --locked --path .

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
