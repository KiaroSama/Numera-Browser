use super::*;
use numera_core::ownership::{capture, resources};
use std::{process::Command, time::Duration};
fn planted(kind: &str, gated: bool) {
    let dir = fresh();
    let mut c = Command::new(support::fixture());
    c.arg("resource")
        .arg(dir.path())
        .arg(kind)
        .arg(if gated { "on" } else { "off" });
    let mut natural = None;
    let mut record = None;
    let out = capture::run_observed(
        &mut c,
        support::limit(Duration::from_secs(5)),
        || false,
        |event, _| {
            if let capture::Observation::BeforeCleanup {
                record: r,
                leader_exited,
                natural_settlement,
            } = event
            {
                record = Some(r);
                natural = Some(resources::require_natural_settlement(
                    leader_exited,
                    natural_settlement,
                ));
            }
        },
    )
    .expect("browser contract");
    let verdict = out.exit == 0 && (!gated || natural.as_ref().expect("browser contract").is_ok());
    if gated && kind != "clean" {
        assert!(!verdict, "planted {kind} escaped native resource gate");
        if kind == "file" {
            assert!(String::from_utf8_lossy(&out.stderr).contains("handles_increased=true"));
        }
        if kind == "thread" {
            assert!(String::from_utf8_lossy(&out.stderr).contains("panics=1"));
        }
        if kind == "child" {
            assert!(
                natural.expect("browser contract").is_err(),
                "live descendant was not observed before cleanup"
            );
        }
    } else {
        assert!(
            verdict,
            "clean/control fixture failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(
        numera_core::ownership::tree::record_empty(&record.expect("browser contract"))
            .expect("browser contract"),
        "planted fixture survived gate cleanup"
    );
}
source_test!(
    test_the_gate_fails_a_planted_leak_running_subprocess,
    "tests/test_warning_gate.py::test_the_gate_fails_a_planted_leak[running subprocess]",
    {
        planted("child", true);
    }
);
source_test!(
    test_the_gate_fails_a_planted_leak_thread_failure,
    "tests/test_warning_gate.py::test_the_gate_fails_a_planted_leak[thread failure]",
    {
        planted("thread", true);
    }
);
source_test!(
    test_the_gate_fails_a_planted_leak_unclosed_file,
    "tests/test_warning_gate.py::test_the_gate_fails_a_planted_leak[unclosed file]",
    {
        planted("file", true);
    }
);
source_test!(
    test_the_planted_leak_is_otherwise_a_passing_test_running_subprocess,
    "tests/test_warning_gate.py::test_the_planted_leak_is_otherwise_a_passing_test[running subprocess]",
    {
        planted("child", false);
    }
);
source_test!(
    test_the_planted_leak_is_otherwise_a_passing_test_thread_failure,
    "tests/test_warning_gate.py::test_the_planted_leak_is_otherwise_a_passing_test[thread failure]",
    {
        planted("thread", false);
    }
);
source_test!(
    test_the_planted_leak_is_otherwise_a_passing_test_unclosed_file,
    "tests/test_warning_gate.py::test_the_planted_leak_is_otherwise_a_passing_test[unclosed file]",
    {
        planted("file", false);
    }
);
source_test!(
    test_a_clean_test_still_passes_under_the_gate,
    "tests/test_warning_gate.py::test_a_clean_test_still_passes_under_the_gate",
    {
        planted("clean", true);
    }
);
