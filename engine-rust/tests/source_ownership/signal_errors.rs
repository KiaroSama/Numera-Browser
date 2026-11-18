//! Same-case controls for a refused signal followed by authoritative observation.
use super::*;
use numera_core::ownership::test_support::{Backend, with_backend};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Native;
impl Backend for Native {}
#[derive(Clone, Copy)]
enum Outcome {
    NaturalExit,
    Live,
    QueryError,
}
struct Refused {
    record: Value,
    release: PathBuf,
    outcome: Outcome,
    signals: AtomicUsize,
    released: AtomicBool,
}
impl Backend for Refused {
    fn signal(&self, target: i32, force: bool) -> Option<io::Result<()>> {
        assert_eq!(
            i64::from(target),
            -self.record["tree"]["group"].as_i64().expect("group")
        );
        assert!(force);
        self.signals.fetch_add(1, Ordering::SeqCst);
        Some(Err(io::Error::from_raw_os_error(libc::EPERM)))
    }
    fn group_empty(&self, group: i32) -> Option<io::Result<bool>> {
        if self.record["tree"]["group"].as_i64() != Some(i64::from(group)) {
            return None;
        }
        let signalled = self.signals.load(Ordering::SeqCst) > 0;
        if signalled && matches!(self.outcome, Outcome::QueryError) {
            return Some(Err(io::Error::from_raw_os_error(libc::EIO)));
        }
        let observed = with_backend(Arc::new(Native), || recorded_tree::empty(&self.record));
        if signalled
            && matches!(self.outcome, Outcome::NaturalExit)
            && matches!(&observed, Ok(false))
            && !self.released.swap(true, Ordering::SeqCst)
            && let Err(error) = fs::write(&self.release, b"go")
        {
            return Some(Err(error));
        }
        // The refused-signal snapshot is genuinely nonempty. Only after it
        // has been taken does the first control let the real child exit.
        Some(observed)
    }
}
pub(super) fn controls() -> TestResult {
    for outcome in [Outcome::NaturalExit, Outcome::Live, Outcome::QueryError] {
        let root = temp("refused-signal-proof")?;
        let ready = root.path().join("ready");
        let release = root.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let record = as_record(&tree);
        let hook = Arc::new(Refused {
            record: record.clone(),
            release: release.clone(),
            outcome,
            signals: AtomicUsize::new(0),
            released: AtomicBool::new(false),
        });
        let budget = if matches!(outcome, Outcome::NaturalExit) {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(40)
        };
        let started = Instant::now();
        let result = with_backend(hook.clone(), || tree.settle(budget));
        assert_eq!(
            hook.signals.load(Ordering::SeqCst),
            1,
            "Refused signal was repeated"
        );
        match outcome {
            Outcome::NaturalExit => {
                result.expect("A refused signal must not discard a later real empty-tree and child-exit proof");
                assert!(hook.released.load(Ordering::SeqCst));
                assert!(recorded_tree::empty(&record)?);
                assert!(tree.child_mut().try_wait()?.is_some());
            }
            Outcome::Live | Outcome::QueryError => {
                let error = result.expect_err("Live or unknown tree was reported settled");
                assert_eq!(
                    error.raw_os_error(),
                    Some(libc::EPERM),
                    "Original raw signal error was replaced"
                );
                assert!(started.elapsed() >= budget);
                assert!(
                    started.elapsed() < Duration::from_secs(1),
                    "Original cleanup deadline was extended"
                );
                assert!(tree.child_mut().try_wait()?.is_none());
                assert!(!recorded_tree::empty(&record)?);
                fs::write(&release, b"go")?;
                tree.settle(Duration::from_secs(2))?;
            }
        }
    }
    Ok(())
}
