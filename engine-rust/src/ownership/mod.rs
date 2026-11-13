//! Native ownership identities; full durable tree lifecycle is still in progress.
pub mod capture;
mod capture_pipe;
pub mod capture_units;
pub mod claims;
pub mod identity;
#[cfg(target_os = "macos")]
mod macos;
pub mod memory;
pub mod observation;
pub mod parent;
pub mod ready;
pub mod recorded_tree;
pub mod registry;
pub mod resources;
pub mod session;
#[cfg(feature = "test-support")]
pub mod test_support;
pub mod tree;
pub mod unit_report;
pub mod update_admission;
#[cfg(test)]
mod update_admission_tests;
#[cfg(windows)]
mod windows;

pub mod file_stat;

#[cfg(test)]
use crate as core;
#[cfg(test)]
#[path = "../../tests/common/ownership_process.rs"]
pub(crate) mod ownership_process;
