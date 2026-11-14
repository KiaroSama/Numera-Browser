use super::*;

source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_797ec26e,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[empty-0]",
    {
        let output = emit("empty", 0)?;
        let expected = Vec::<u8>::new();
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_09e66e7f,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[empty-3]",
    {
        let output = emit("empty", 3)?;
        let expected = Vec::<u8>::new();
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_2d074174,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[tail-0]",
    {
        let output = emit("tail", 0)?;
        let expected = b"tail".to_vec();
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_7bb6b4c1,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[tail-3]",
    {
        let output = emit("tail", 3)?;
        let expected = b"tail".to_vec();
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_3c69e3da,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[unicode-newlines-0]",
    {
        let output = emit("unicode", 0)?;
        let expected = "سلام\nΩ\r\nZ\rW".as_bytes().to_vec();
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_1aae816c,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[unicode-newlines-3]",
    {
        let output = emit("unicode", 3)?;
        let expected = "سلام\nΩ\r\nZ\rW".as_bytes().to_vec();
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_3baae608,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[large-0]",
    {
        let output = emit("large", 0)?;
        let expected = vec![b'x'; 200000];
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_normal_and_nonzero_exit_preserves_both_streams_f26ef1eb,
    "tests/test_helper_lifecycle_matrix.py::test_normal_and_nonzero_exit_preserves_both_streams[large-3]",
    {
        let output = emit("large", 3)?;
        let expected = vec![b'x'; 200000];
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, expected);
        Ok(())
    }
);
source_test!(
    helper_ownership_a_plain_helper_returns_its_whole_output_00056de0,
    "tests/test_helper_ownership.py::test_a_plain_helper_returns_its_whole_output[quick-0-ok\\n]",
    {
        let output = emit("ok\n", 0)?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, "ok\n".as_bytes());
        Ok(())
    }
);
source_test!(
    helper_ownership_a_plain_helper_returns_its_whole_output_a377be89,
    "tests/test_helper_ownership.py::test_a_plain_helper_returns_its_whole_output[unhealthy-3-{\"ok\": false}]",
    {
        let output = emit("{\"ok\": false}", 3)?;
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, "{\"ok\": false}".as_bytes());
        Ok(())
    }
);
source_test!(
    helper_deadline_a_helper_that_finishes_is_untouched,
    "tests/test_helper_deadline.py::test_a_helper_that_finishes_is_untouched",
    {
        let output = emit("done\n", 0)?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, "done\n".as_bytes());
        Ok(())
    }
);
source_test!(
    helper_deadline_a_failing_helper_still_returns_its_output,
    "tests/test_helper_deadline.py::test_a_failing_helper_still_returns_its_output",
    {
        let output = emit("{\"ok\": false}\n", 3)?;
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, "{\"ok\": false}\n".as_bytes());
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_failing_helper_still_returns_its_output,
    "tests/test_helper_terminal_state.py::test_a_failing_helper_still_returns_its_output",
    {
        let output = emit("{\"ok\": false}", 3)?;
        assert_eq!(output.exit, 3);
        assert_eq!(output.stdout, "{\"ok\": false}".as_bytes());
        Ok(())
    }
);
source_test!(
    helper_deadline_a_wedged_helper_is_ended_rather_than_abandoned,
    "tests/test_helper_deadline.py::test_a_wedged_helper_is_ended_rather_than_abandoned",
    {
        let t = temp("wedged-helper-tree")?;
        let mut child = command("tree")?;
        child.arg(t.path()).arg("hold");
        let start = Instant::now();
        let failure = capture::run(&mut child, limit(Duration::from_millis(350)))
            .err()
            .ok_or("wedged helper did not time out")?;
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(
            t.path().join("descendant").exists(),
            "fixture never created descendant"
        );
        let leader: i64 = fs::read_to_string(t.path().join("leader"))?.parse()?;
        #[cfg(unix)]
        assert!(numera_core::ownership::tree::record_empty(
            &json!({"tree":{"kind":"pgid","group":leader}})
        )?);
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_wedged_helper_times_out_and_is_ended,
    "tests/test_helper_terminal_state.py::test_a_wedged_helper_times_out_and_is_ended",
    {
        let t = temp("wedged-helper-tree")?;
        let mut child = command("tree")?;
        child.arg(t.path()).arg("hold");
        let start = Instant::now();
        let failure = capture::run(&mut child, limit(Duration::from_millis(350)))
            .err()
            .ok_or("wedged helper did not time out")?;
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(
            t.path().join("descendant").exists(),
            "fixture never created descendant"
        );
        let leader: i64 = fs::read_to_string(t.path().join("leader"))?.parse()?;
        #[cfg(unix)]
        assert!(numera_core::ownership::tree::record_empty(
            &json!({"tree":{"kind":"pgid","group":leader}})
        )?);
        Ok(())
    }
);
source_test!(
    helper_ownership_a_wedged_helper_with_a_driver_times_out_within_the_declared_bound,
    "tests/test_helper_ownership.py::test_a_wedged_helper_with_a_driver_times_out_within_the_declared_bound",
    {
        let t = temp("wedged-helper-tree")?;
        let mut child = command("tree")?;
        child.arg(t.path()).arg("hold");
        // The source BUDGET_WEDGED includes helper and descendant startup;
        // its CLEANUP_BUDGET is five seconds. Neither is a product deadline.
        let wall = Duration::from_secs(3);
        let cleanup = Duration::from_secs(5);
        let start = Instant::now();
        let mut tail = Vec::new();
        let mut ready_at = None;
        let failure = capture::run_observed_streamed(
            &mut child,
            capture::Limits {
                cleanup,
                ..limit(wall)
            },
            || false,
            |_, _| {},
            |stream, bytes| {
                if stream == 0 {
                    tail.extend_from_slice(bytes);
                    if tail == b"TAIL" && t.path().join("descendant").is_file() {
                        ready_at = Some(start.elapsed());
                    }
                }
            },
        )
        .err()
        .ok_or("wedged helper did not time out")?;
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() >= wall);
        assert!(start.elapsed() < wall + cleanup + Duration::from_secs(1));
        assert_eq!(tail, b"TAIL");
        assert!(ready_at.is_some_and(|elapsed| elapsed < wall));
        assert!(
            t.path().join("descendant").exists(),
            "fixture never created descendant"
        );
        let leader: i64 = fs::read_to_string(t.path().join("leader"))?.parse()?;
        #[cfg(unix)]
        assert!(numera_core::ownership::tree::record_empty(
            &json!({"tree":{"kind":"pgid","group":leader}})
        )?);
        Ok(())
    }
);
source_test!(
    helper_deadline_the_helper_runs_in_a_tree_that_can_be_ended,
    "tests/test_helper_deadline.py::test_the_helper_runs_in_a_tree_that_can_be_ended",
    {
        let output = {
            let mut child = command("owned-exit")?;
            capture::run(&mut child, limit(Duration::from_secs(5)))?
        };
        assert_eq!(output.exit, 0);
        let pid: u32 = String::from_utf8(output.stdout)?.trim().parse()?;
        assert!(pid > 0);
        assert!(!identity::alive(pid, None)?);
        Ok(())
    }
);
source_test!(
    helper_ownership_process_observation_retains_identity_and_rejects_survivors_b5bcdf45,
    "tests/test_helper_ownership.py::test_process_observation_retains_identity_and_rejects_survivors[True-0.05]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Probe {
            clock: Arc<AtomicU64>,
            calls: std::sync::Mutex<Vec<(u32, Option<u64>)>>,
        }
        impl Backend for Probe {
            fn alive(&self, pid: u32, token: Option<u64>) -> Option<io::Result<bool>> {
                self.calls.lock().expect("observations").push((pid, token));
                Some(Ok(!true || self.clock.load(Ordering::SeqCst) < 20_000_000))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                panic!("observation attempted termination")
            }
        }
        let clock = Arc::new(AtomicU64::new(0));
        let backend = Arc::new(Probe {
            clock: clock.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let mut waits = Vec::new();
        let budget = 50000000u64;
        let success = with_backend(backend.clone(), || -> io::Result<bool> {
            loop {
                if !identity::alive(123, Some(456))? {
                    return Ok(true);
                }
                let elapsed = clock.load(Ordering::SeqCst);
                if elapsed >= budget {
                    return Ok(false);
                }
                let step = (10_000_000u64).min(budget.saturating_sub(elapsed));
                assert!(step > 0);
                waits.push(step);
                clock.fetch_add(step, Ordering::SeqCst);
            }
        })?;
        assert!(success);
        assert!(!waits.is_empty());
        assert!(clock.load(Ordering::SeqCst) <= 50000000);
        let calls = backend.calls.lock().expect("calls");
        assert!(!calls.is_empty());
        assert!(calls.iter().all(|call| *call == (123, Some(456))));
        Ok(())
    }
);
source_test!(
    helper_ownership_process_observation_retains_identity_and_rejects_survivors_7c1902f7,
    "tests/test_helper_ownership.py::test_process_observation_retains_identity_and_rejects_survivors[False-0.05]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Probe {
            clock: Arc<AtomicU64>,
            calls: std::sync::Mutex<Vec<(u32, Option<u64>)>>,
        }
        impl Backend for Probe {
            fn alive(&self, pid: u32, token: Option<u64>) -> Option<io::Result<bool>> {
                self.calls.lock().expect("observations").push((pid, token));
                Some(Ok(!false || self.clock.load(Ordering::SeqCst) < 20_000_000))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                panic!("observation attempted termination")
            }
        }
        let clock = Arc::new(AtomicU64::new(0));
        let backend = Arc::new(Probe {
            clock: clock.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let mut waits = Vec::new();
        let budget = 50000000u64;
        let success = with_backend(backend.clone(), || -> io::Result<bool> {
            loop {
                if !identity::alive(123, Some(456))? {
                    return Ok(true);
                }
                let elapsed = clock.load(Ordering::SeqCst);
                if elapsed >= budget {
                    return Ok(false);
                }
                let step = (10_000_000u64).min(budget.saturating_sub(elapsed));
                assert!(step > 0);
                waits.push(step);
                clock.fetch_add(step, Ordering::SeqCst);
            }
        })?;
        assert!(!success);
        assert!(!waits.is_empty());
        assert!(clock.load(Ordering::SeqCst) <= 50000000);
        let calls = backend.calls.lock().expect("calls");
        assert!(!calls.is_empty());
        assert!(calls.iter().all(|call| *call == (123, Some(456))));
        Ok(())
    }
);
source_test!(
    helper_ownership_process_observation_retains_identity_and_rejects_survivors_c68f7c85,
    "tests/test_helper_ownership.py::test_process_observation_retains_identity_and_rejects_survivors[False-0.005]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Probe {
            clock: Arc<AtomicU64>,
            calls: std::sync::Mutex<Vec<(u32, Option<u64>)>>,
        }
        impl Backend for Probe {
            fn alive(&self, pid: u32, token: Option<u64>) -> Option<io::Result<bool>> {
                self.calls.lock().expect("observations").push((pid, token));
                Some(Ok(!false || self.clock.load(Ordering::SeqCst) < 20_000_000))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                panic!("observation attempted termination")
            }
        }
        let clock = Arc::new(AtomicU64::new(0));
        let backend = Arc::new(Probe {
            clock: clock.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let mut waits = Vec::new();
        let budget = 5000000u64;
        let success = with_backend(backend.clone(), || -> io::Result<bool> {
            loop {
                if !identity::alive(123, Some(456))? {
                    return Ok(true);
                }
                let elapsed = clock.load(Ordering::SeqCst);
                if elapsed >= budget {
                    return Ok(false);
                }
                let step = (10_000_000u64).min(budget.saturating_sub(elapsed));
                assert!(step > 0);
                waits.push(step);
                clock.fetch_add(step, Ordering::SeqCst);
            }
        })?;
        assert!(!success);
        assert!(!waits.is_empty());
        assert!(clock.load(Ordering::SeqCst) <= 5000000);
        let calls = backend.calls.lock().expect("calls");
        assert!(!calls.is_empty());
        assert!(calls.iter().all(|call| *call == (123, Some(456))));
        Ok(())
    }
);
source_test!(
    helper_ownership_process_observation_retains_identity_and_rejects_survivors_3491f79f,
    "tests/test_helper_ownership.py::test_process_observation_retains_identity_and_rejects_survivors[False-0]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Probe {
            clock: Arc<AtomicU64>,
            calls: std::sync::Mutex<Vec<(u32, Option<u64>)>>,
        }
        impl Backend for Probe {
            fn alive(&self, pid: u32, token: Option<u64>) -> Option<io::Result<bool>> {
                self.calls.lock().expect("observations").push((pid, token));
                Some(Ok(!false || self.clock.load(Ordering::SeqCst) < 20_000_000))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                panic!("observation attempted termination")
            }
        }
        let clock = Arc::new(AtomicU64::new(0));
        let backend = Arc::new(Probe {
            clock: clock.clone(),
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let mut waits = Vec::new();
        let budget = 0u64;
        let success = with_backend(backend.clone(), || -> io::Result<bool> {
            loop {
                if !identity::alive(123, Some(456))? {
                    return Ok(true);
                }
                let elapsed = clock.load(Ordering::SeqCst);
                if elapsed >= budget {
                    return Ok(false);
                }
                let step = (10_000_000u64).min(budget.saturating_sub(elapsed));
                assert!(step > 0);
                waits.push(step);
                clock.fetch_add(step, Ordering::SeqCst);
            }
        })?;
        assert!(!success);
        assert!(waits.is_empty());
        assert!(clock.load(Ordering::SeqCst) == 0);
        let calls = backend.calls.lock().expect("calls");
        assert!(!calls.is_empty());
        assert!(calls.iter().all(|call| *call == (123, Some(456))));
        Ok(())
    }
);
source_test!(helper_lifecycle_matrix_fixture_process_observation_preserves_identity_deadline_and_survivor_failure_b5bcdf45,"tests/test_helper_lifecycle_matrix.py::test_fixture_process_observation_preserves_identity_deadline_and_survivor_failure[True-0.05]",{
use numera_core::ownership::test_support::{Backend,with_backend};use std::sync::atomic::{AtomicU64,Ordering};
struct Probe{clock:Arc<AtomicU64>,calls:std::sync::Mutex<Vec<(u32,Option<u64>)>>}impl Backend for Probe{fn alive(&self,pid:u32,token:Option<u64>)->Option<io::Result<bool>>{self.calls.lock().expect("observations").push((pid,token));Some(Ok(!true||self.clock.load(Ordering::SeqCst)<20_000_000))}fn signal(&self,_:i32,_:bool)->Option<io::Result<()>>{panic!("observation attempted termination")}}
let clock=Arc::new(AtomicU64::new(0));let backend=Arc::new(Probe{clock:clock.clone(),calls:std::sync::Mutex::new(Vec::new())});let mut waits=Vec::new();let budget=50000000u64;let success=with_backend(backend.clone(),||->io::Result<bool>{loop{if !identity::alive(123,Some(456))?{return Ok(true);}let elapsed=clock.load(Ordering::SeqCst);if elapsed>=budget{return Ok(false);}let step=(10_000_000u64).min(budget.saturating_sub(elapsed));assert!(step>0);waits.push(step);clock.fetch_add(step,Ordering::SeqCst);}})?;
assert!(success);assert!(!waits.is_empty());assert!(clock.load(Ordering::SeqCst)<=50000000);let calls=backend.calls.lock().expect("calls");assert!(!calls.is_empty());assert!(calls.iter().all(|call|*call==(123,Some(456))));Ok(())
});
source_test!(helper_lifecycle_matrix_fixture_process_observation_preserves_identity_deadline_and_survivor_failure_7c1902f7,"tests/test_helper_lifecycle_matrix.py::test_fixture_process_observation_preserves_identity_deadline_and_survivor_failure[False-0.05]",{
use numera_core::ownership::test_support::{Backend,with_backend};use std::sync::atomic::{AtomicU64,Ordering};
struct Probe{clock:Arc<AtomicU64>,calls:std::sync::Mutex<Vec<(u32,Option<u64>)>>}impl Backend for Probe{fn alive(&self,pid:u32,token:Option<u64>)->Option<io::Result<bool>>{self.calls.lock().expect("observations").push((pid,token));Some(Ok(!false||self.clock.load(Ordering::SeqCst)<20_000_000))}fn signal(&self,_:i32,_:bool)->Option<io::Result<()>>{panic!("observation attempted termination")}}
let clock=Arc::new(AtomicU64::new(0));let backend=Arc::new(Probe{clock:clock.clone(),calls:std::sync::Mutex::new(Vec::new())});let mut waits=Vec::new();let budget=50000000u64;let success=with_backend(backend.clone(),||->io::Result<bool>{loop{if !identity::alive(123,Some(456))?{return Ok(true);}let elapsed=clock.load(Ordering::SeqCst);if elapsed>=budget{return Ok(false);}let step=(10_000_000u64).min(budget.saturating_sub(elapsed));assert!(step>0);waits.push(step);clock.fetch_add(step,Ordering::SeqCst);}})?;
assert!(!success);assert!(!waits.is_empty());assert!(clock.load(Ordering::SeqCst)<=50000000);let calls=backend.calls.lock().expect("calls");assert!(!calls.is_empty());assert!(calls.iter().all(|call|*call==(123,Some(456))));Ok(())
});
source_test!(helper_lifecycle_matrix_fixture_process_observation_preserves_identity_deadline_and_survivor_failure_c68f7c85,"tests/test_helper_lifecycle_matrix.py::test_fixture_process_observation_preserves_identity_deadline_and_survivor_failure[False-0.005]",{
use numera_core::ownership::test_support::{Backend,with_backend};use std::sync::atomic::{AtomicU64,Ordering};
struct Probe{clock:Arc<AtomicU64>,calls:std::sync::Mutex<Vec<(u32,Option<u64>)>>}impl Backend for Probe{fn alive(&self,pid:u32,token:Option<u64>)->Option<io::Result<bool>>{self.calls.lock().expect("observations").push((pid,token));Some(Ok(!false||self.clock.load(Ordering::SeqCst)<20_000_000))}fn signal(&self,_:i32,_:bool)->Option<io::Result<()>>{panic!("observation attempted termination")}}
let clock=Arc::new(AtomicU64::new(0));let backend=Arc::new(Probe{clock:clock.clone(),calls:std::sync::Mutex::new(Vec::new())});let mut waits=Vec::new();let budget=5000000u64;let success=with_backend(backend.clone(),||->io::Result<bool>{loop{if !identity::alive(123,Some(456))?{return Ok(true);}let elapsed=clock.load(Ordering::SeqCst);if elapsed>=budget{return Ok(false);}let step=(10_000_000u64).min(budget.saturating_sub(elapsed));assert!(step>0);waits.push(step);clock.fetch_add(step,Ordering::SeqCst);}})?;
assert!(!success);assert!(!waits.is_empty());assert!(clock.load(Ordering::SeqCst)<=5000000);let calls=backend.calls.lock().expect("calls");assert!(!calls.is_empty());assert!(calls.iter().all(|call|*call==(123,Some(456))));Ok(())
});
source_test!(helper_lifecycle_matrix_fixture_process_observation_preserves_identity_deadline_and_survivor_failure_a5ebd0df,"tests/test_helper_lifecycle_matrix.py::test_fixture_process_observation_preserves_identity_deadline_and_survivor_failure[False-0.0]",{
use numera_core::ownership::test_support::{Backend,with_backend};use std::sync::atomic::{AtomicU64,Ordering};
struct Probe{clock:Arc<AtomicU64>,calls:std::sync::Mutex<Vec<(u32,Option<u64>)>>}impl Backend for Probe{fn alive(&self,pid:u32,token:Option<u64>)->Option<io::Result<bool>>{self.calls.lock().expect("observations").push((pid,token));Some(Ok(!false||self.clock.load(Ordering::SeqCst)<20_000_000))}fn signal(&self,_:i32,_:bool)->Option<io::Result<()>>{panic!("observation attempted termination")}}
let clock=Arc::new(AtomicU64::new(0));let backend=Arc::new(Probe{clock:clock.clone(),calls:std::sync::Mutex::new(Vec::new())});let mut waits=Vec::new();let budget=0u64;let success=with_backend(backend.clone(),||->io::Result<bool>{loop{if !identity::alive(123,Some(456))?{return Ok(true);}let elapsed=clock.load(Ordering::SeqCst);if elapsed>=budget{return Ok(false);}let step=(10_000_000u64).min(budget.saturating_sub(elapsed));assert!(step>0);waits.push(step);clock.fetch_add(step,Ordering::SeqCst);}})?;
assert!(!success);assert!(waits.is_empty());assert!(clock.load(Ordering::SeqCst)==0);let calls=backend.calls.lock().expect("calls");assert!(!calls.is_empty());assert!(calls.iter().all(|call|*call==(123,Some(456))));Ok(())
});
