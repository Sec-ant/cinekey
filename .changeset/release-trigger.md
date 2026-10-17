---
default: patch
---

# Publish when the Release PR merges

Merging the Release PR starts an observable Actions workflow that prepares the
version tag and launches publishing for the crate, npm package, and release assets.
