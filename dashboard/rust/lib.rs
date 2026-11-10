//! Browser application and its domain model share the same Rust implementation.
//! JSON records retain unknown fields so configuration exports are lossless.
#[cfg(target_arch = "wasm32")]
pub mod browser;
pub mod contracts;
#[cfg(not(target_arch = "wasm32"))]
pub mod development;
pub mod model;
pub mod render;
pub mod updates;
