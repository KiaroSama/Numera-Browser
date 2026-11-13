//! Browser-realm application logic. The distribution includes only compiler-
//! generated wasm-bindgen JavaScript; all authored behavior lives in Rust.
#[cfg(not(target_arch = "wasm32"))]
pub mod artifact;
pub mod identity;
pub mod probes;
pub mod webrtc_rules;
