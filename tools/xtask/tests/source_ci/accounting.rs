use super::*;
use numera_core::ownership::memory::{self as native, Accounting};
use std::{cell::RefCell, rc::Rc};
struct Api {
    damage: &'static str,
    calls: Rc<RefCell<Vec<&'static str>>>,
    observations: usize,
}
impl Api {
    fn new(damage: &'static str) -> Self {
        Self {
            damage,
            calls: Rc::new(RefCell::new(Vec::new())),
            observations: 0,
        }
    }
}
impl accounting::Backend for Api {
    fn begin(&mut self) -> ci::Result<bool> {
        self.calls.borrow_mut().push("open");
        Ok(self.damage != "open")
    }
    fn snapshot(&mut self) -> ci::Result<Accounting> {
        self.calls.borrow_mut().push("membership");
        if ["foreign", "membership"].contains(&self.damage) {
            return Err(io::Error::other("private job name").into());
        }
        self.calls.borrow_mut().push("query");
        self.observations += 1;
        if self.damage == "query" || self.damage == "after-query" && self.observations == 2 {
            return Err(io::Error::other("private query error").into());
        }
        let value = if self.damage == "decreased" && self.observations == 2 {
            1
        } else {
            self.observations as u64 * 10
        };
        let mut bytes = [0u8; 96];
        for (offset, number) in [(0, value), (8, value + 1)] {
            bytes[offset..offset + 8].copy_from_slice(
                &(if self.damage == "negative" {
                    -1i64
                } else {
                    number as i64
                })
                .to_le_bytes(),
            );
        }
        for (offset, number) in [
            (32, value + 2),
            (36, value + 3),
            (
                40,
                if self.damage == "active" {
                    value + 4
                } else {
                    1
                },
            ),
            (44, 0),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&(number as u32).to_le_bytes());
        }
        for (index, offset) in (48..96).step_by(8).enumerate() {
            bytes[offset..offset + 8].copy_from_slice(&(value + index as u64).to_le_bytes());
        }
        Ok(native::accounting_from_bytes(
            &bytes,
            if self.damage == "short" { 95 } else { 96 },
        )?)
    }
    fn finish(&mut self) -> ci::Result<()> {
        self.calls.borrow_mut().push("close");
        if self.damage == "close" {
            Err(io::Error::other("private close error").into())
        } else {
            Ok(())
        }
    }
}
fn counters(damage: &'static str) -> TestResult {
    let backend = Api::new(damage);
    let calls = backend.calls.clone();
    let mut reached = false;
    let (result, events) = accounting::measure(
        7,
        backend,
        100,
        || 210,
        || {
            reached = true;
            Ok(())
        },
    );
    result?;
    assert!(reached);
    assert_eq!(events.len(), 1);
    if damage == "None" {
        assert!(
            events[0].contains(
                "index=7 coverage=owned-proof-scope-not-compiler-attribution wall_ns=110"
            )
        );
        let values: Value =
            serde_json::from_str(events[0].split(" counters=").nth(1).expect("counters"))?;
        assert_eq!(
            values.as_object().expect("object").len(),
            native::ACCOUNTING_COUNTERS.len()
        );
        for key in native::ACCOUNTING_COUNTERS {
            assert_eq!(
                values[key],
                if key == "terminated_processes" { 0 } else { 10 }
            );
        }
        assert!(events[0].contains("active_before=1 active_after=1"));
        assert_eq!(
            *calls.borrow(),
            vec![
                "open",
                "membership",
                "query",
                "membership",
                "query",
                "close"
            ]
        );
    } else {
        assert_eq!(
            events,
            vec!["Historical resources index=7 status=unavailable"]
        );
    }
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|call| **call == "close")
            .count(),
        usize::from(damage != "open")
    );
    if ["foreign", "membership"].contains(&damage) {
        assert!(!calls.borrow().contains(&"query"));
    }
    assert!(events.iter().all(|event| !event.contains("private")));
    Ok(())
}
source_cases!("tests/test_native_history_resources.py::test_accounting_checks_membership_layout_and_monotonicity_without_payloads",counters,[accounting_good=>("None","None"),accounting_open=>("open","open"),accounting_foreign=>("foreign","foreign"),accounting_membership=>("membership","membership"),accounting_query=>("query","query"),accounting_short=>("short","short"),accounting_negative=>("negative","negative"),accounting_active=>("active","active"),accounting_after=>("after-query","after-query"),accounting_decreased=>("decreased","decreased")]);
fn unavailable((platform, inherited): (&str, bool)) -> TestResult {
    let api = Api::new("None");
    let calls = api.calls.clone();
    assert!(accounting::available(platform, inherited).is_err());
    assert!(calls.borrow().is_empty());
    Ok(())
}
source_cases!("tests/test_native_history_resources.py::test_unavailable_scope_never_opens_or_adopts_another_job",unavailable,[accounting_linux_unavailable=>("linux-True",("linux",true)),accounting_windows_missing=>("win32-False",("win32",false))]);
fn close(failure: bool) -> TestResult {
    let api = Api::new("close");
    let calls = api.calls.clone();
    let (result, events) = accounting::measure(
        1,
        api,
        100,
        || 210,
        || {
            if failure {
                Err::<(), _>(
                    io::Error::new(io::ErrorKind::TimedOut, "fixture proof deadline").into(),
                )
            } else {
                Ok(())
            }
        },
    );
    let error = result.expect_err("owned close failure must fail");
    if failure {
        assert_eq!(
            error
                .downcast_ref::<io::Error>()
                .expect("original IO error")
                .kind(),
            io::ErrorKind::TimedOut
        );
    } else {
        assert_eq!(
            error.to_string(),
            "Historical resource query handle did not close"
        );
    }
    assert_eq!(calls.borrow().last(), Some(&"close"));
    assert!(
        events
            .iter()
            .any(|event| event.contains("status=handle-close-failed"))
    );
    Ok(())
}
source_cases!("tests/test_native_history_resources.py::test_failed_query_handle_close_is_fatal_without_replacing_an_existing_proof_error",close,[accounting_close_control=>("False",false),accounting_close_prior=>("True",true)]);
fn public_runner(failure: bool) -> TestResult {
    let temp = Temp::new("history-accounting")?;
    let source = temp.write("engine-rust/src/models.rs", b"fixed")?;
    let historical = b"previous defect";
    let mut case = history::Registry::read()?.cases()?[0].clone();
    case.files = BTreeMap::from([("engine-rust/src/models.rs".into(), ci::sha(historical))]);
    case.companions.clear();
    let mut inputs = BTreeMap::from([(
        (
            case.commit.clone(),
            "engine-rust/src/models.rs".into(),
            ci::sha(historical),
        ),
        historical.to_vec(),
    )]);
    let calls = RefCell::new(Vec::new());
    let (result, events) = accounting::measure(
        0,
        Api::new("None"),
        100,
        || 210,
        || {
            history::prove_cases_with(
                &temp.0,
                std::slice::from_ref(&case),
                "linux",
                &mut inputs,
                |compile, case, command| {
                    assert_eq!(fs::read(&source)?, historical);
                    let args = command
                        .get_args()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    assert!(
                        args.contains(&"--locked".into())
                            && args.contains(&"--offline".into())
                            && args.contains(&"--timings".into())
                    );
                    calls
                        .borrow_mut()
                        .push(if compile { "compile" } else { "proof" });
                    if !compile && failure {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "fixture proof failure",
                        )
                        .into());
                    }
                    let text = if compile {
                        String::new()
                    } else {
                        failed_transcript(&case.test, &case.symptom)
                    };
                    Ok(ci::owned::Output {
                        command: numera_core::ownership::capture::Output {
                            exit: if compile { 0 } else { 101 },
                            stdout: text.into_bytes(),
                            stderr: Vec::new(),
                        },
                        memory: Value::Null,
                    })
                },
            )
        },
    );
    assert_eq!(result.is_err(), failure);
    assert_eq!(*calls.borrow(), vec!["compile", "proof"]);
    assert_eq!(fs::read(source)?, b"fixed");
    assert_eq!(events.len(), 1);
    Ok(())
}
source_cases!("tests/test_native_history_resources.py::test_public_historical_runner_keeps_exact_commands_restoration_and_original_errors",public_runner,[accounting_runner_good=>("False",false),accounting_runner_timeout=>("True",true)]);
source_test!(
    accounting_real_windows,
    "tests/test_native_history_resources.py::test_real_windows_inherited_job_reports_numeric_lifetime_accounting",
    {
        #[cfg(windows)]
        {
            let temp = Temp::new("accounting-os")?;
            let mut tree = numera_core::ownership::tree::OwnedTree::spawn(
                std::process::Command::new("cmd.exe").args([
                    "/d",
                    "/c",
                    "ping -n 4 127.0.0.1 >nul",
                ]),
            )?;
            let backend = accounting::Native::new(&tree)?;
            let (result, events) = accounting::measure(
                0,
                backend,
                100,
                || 210,
                || {
                    temp.write("accounting-fixture", &(0..=255).collect::<Vec<u8>>())?;
                    Ok(())
                },
            );
            tree.settle(std::time::Duration::from_secs(5))?;
            result?;
            assert_eq!(events.len(), 1);
            assert!(events[0].contains("coverage=owned-proof-scope-not-compiler-attribution"));
            let values: Value =
                serde_json::from_str(events[0].split(" counters=").nth(1).expect("counters"))?;
            assert!(
                native::ACCOUNTING_COUNTERS
                    .iter()
                    .all(|key| values[*key].as_u64().is_some())
            );
        }
        #[cfg(not(windows))]
        {
            assert!(accounting::available(std::env::consts::OS, true).is_err());
        }
        Ok(())
    }
);
