---
default: minor
---

# Extract video bit depth

Expose `video_bit_depth` in Rust, CLI, and Wasm output for explicit 8-, 10-, and
12-bit release markers, including `Hi10P`. Matching markers share one numeric
value; conflicting markers leave the field absent.
