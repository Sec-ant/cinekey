#![forbid(unsafe_code)]

//! Browser and Node bindings for cinekey's string-based parsing API.
//!
//! `parse` and `parseWithContext` return plain JavaScript metadata objects.

#[cfg(target_arch = "wasm32")]
mod bindings;
