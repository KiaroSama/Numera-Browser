//! Native persistence adapters; application reconciliation remains separately gated.
mod archive;
pub(crate) mod archive_paths;
pub mod atomic;
mod backup;
pub mod config;
pub mod dashboard;
mod dashboard_revision;
pub mod data_root;
pub mod extension_install;
pub mod extension_package;
pub mod extensions;
pub mod first_run;
#[cfg(target_os = "linux")]
mod first_run_xattrs;
pub mod folders;
pub mod locks;
pub mod paths;
pub mod pool;
pub mod repo;
mod repo_delete;
mod repo_lookup;
mod repo_reconcile;
mod repo_write;
mod restore;
pub mod schedules;
