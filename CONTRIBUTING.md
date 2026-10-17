# Contributing

Install [Knope](https://knope.tech/installation/) 0.23.0 to create change files:

```sh
brew install knope-dev/tap/knope
```

Cargo also provides installation:

```sh
cargo install knope --version 0.23.0 --locked
```

## Changesets

Run `just change` to select a change type and describe its effect on users. Commit
the generated Markdown file with the implementation. Change files can also be
written directly:

```markdown
---
default: patch
---

# Preserve internal hyphens in release group names
```

`default` names the single cinekey release, including the Rust library, CLI, and
Wasm npm package. Use `patch` for fixes, `minor` for features, and `major` for
breaking changes. While the project is at `0.x`, Knope advances the minor number
for breaking changes and the patch number for features and fixes.

Write release notes that explain the observable change, with migration guidance
for breaking changes. Maintenance changes that affect only development can be
committed directly.

The [Knope Bot GitHub App](https://github.com/apps/knope-bot) is installed for this
repository. After changes reach `main`, it maintains a Release PR with the next
version and changelog. Merging that PR starts Actions, which tags the merged
commit, builds artifacts, and publishes to crates.io, npm, and GitHub Releases.
