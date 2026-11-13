//! Narrow SDK transport; effective launch/application services remain native.
pub mod adapter;
pub mod admission;
pub mod alignment;
pub mod bundled;
pub mod camoufox_fonts;
pub mod container_smoke;
pub(crate) mod cookie_expiry;
pub mod cookie_format;
pub mod cookie_normalize;
pub mod cookies;
pub mod extensions;
pub mod hints;
pub mod ip_identity;
pub mod launch;
pub mod network_grades;
pub mod network_probe;
pub mod options;
pub mod probe_grades;
pub mod probes;
pub mod registry;
pub mod resources;
pub mod runtime;
#[cfg(feature = "native-browser")]
pub mod sdk;
pub mod startup;
pub mod trust_grades;
pub mod trust_probe;
pub mod warmup;
pub mod warmup_policy;
#[cfg(test)]
mod warmup_tests;
