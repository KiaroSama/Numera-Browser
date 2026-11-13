//! Individually reported native ports of the maintained build/CI source cases.
use numera_xtask::ci::{self, accounting, evidence, history, memory, release, scope, workflow};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
type TestResult = ci::Result<()>;
macro_rules! source_test {($name:ident,$id:expr,$body:block)=>{#[test]#[doc=$id]fn $name()->TestResult $body};}
macro_rules! source_cases {
    ($prefix:literal,$helper:ident,[$($name:ident=>($parameter:literal,$args:expr)),*$(,)?])=>{$(source_test!($name,concat!($prefix,"[",$parameter,"]"),{$helper($args)});)*};
}
struct Temp(PathBuf);
impl Temp {
    fn new(label: &str) -> ci::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/source-ci");
        fs::create_dir_all(&parent)?;
        let path = parent.join(format!(
            "{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, bytes: &[u8]) -> ci::Result<PathBuf> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        Ok(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn verdict(result: ci::Result<()>, valid: bool) {
    assert_eq!(
        result.is_ok(),
        valid,
        "unexpected contract verdict: {result:?}"
    );
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[path = "source_ci/accounting.rs"]
mod accounting_cases;
#[path = "source_ci/browser_benchmark.rs"]
mod browser_benchmark;
#[path = "source_ci/camoufox_runtime.rs"]
mod camoufox_runtime;
#[path = "source_ci/candidate.rs"]
mod candidate_cases;
#[path = "source_ci/container.rs"]
mod container_cases;
#[path = "source_ci/desktop_ci.rs"]
mod desktop_ci;
#[path = "source_ci/desktop_history.rs"]
mod desktop_history;
#[path = "source_ci/history_inputs.rs"]
mod history_inputs;
#[path = "source_ci/macos.rs"]
mod macos;
#[path = "source_ci/memory.rs"]
mod memory_cases;
#[path = "source_ci/migration.rs"]
mod migration;
#[path = "source_ci/history.rs"]
mod native_history;
#[path = "source_ci/partition.rs"]
mod partition;
#[path = "source_ci/secret_review.rs"]
mod secret_review;
#[path = "source_ci/version.rs"]
mod version_cases;

#[path = "source_ci/release_distribution.rs"]
mod release_distribution;
#[path = "source_ci/build_identity.rs"]
mod release_identity;
#[path = "source_ci/build_preserves_data.rs"]
mod release_preserves_data;
#[path = "source_ci/release_support.rs"]
mod release_support;

#[path = "source_ci/compiler.rs"]
mod compiler;
