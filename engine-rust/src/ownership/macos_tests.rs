use super::{OBSERVATIONS, Observation};
use crate::ownership::{claims, identity, tree::record_empty};
use serde_json::json;
use std::{collections::VecDeque, io, mem::size_of};

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        OBSERVATIONS.with(|slot| *slot.borrow_mut() = None);
    }
}
fn observe(rows: Vec<Observation>) -> Restore {
    OBSERVATIONS.with(|slot| *slot.borrow_mut() = Some(VecDeque::from(rows)));
    Restore
}
fn consumed() {
    OBSERVATIONS.with(|slot| {
        assert!(slot.borrow().as_ref().is_some_and(VecDeque::is_empty));
    });
}
fn process(pid: u32, group: u32, state: u32) -> libc::proc_bsdinfo {
    // SAFETY: proc_bsdinfo is an integer/byte-array C output record; zero is valid.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    info.pbi_pid = pid;
    info.pbi_pgid = group;
    info.pbi_status = state;
    info.pbi_start_tvsec = 1_700_000_000;
    info.pbi_start_tvusec = 123_456;
    info
}
fn bsd(pid: u32, info: libc::proc_bsdinfo, returned: i32, error: i32) -> Observation {
    Observation::Process {
        pid,
        info,
        returned,
        error,
    }
}
fn list(capacity: usize, pids: &[i32], returned: i32, error: i32) -> Observation {
    Observation::Group {
        group: 123,
        capacity,
        pids: pids.to_vec(),
        returned,
        error,
    }
}
fn record() -> serde_json::Value {
    json!({"pid":123,"token":42,"session":"fixture-session","host":"fixture-host",
           "tree":{"kind":"pgid","group":123}})
}

#[test]
fn macos_public_group_observation_retains_every_uncertain_claim() {
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    let cases = [
        ("empty", vec![list(64, &[], 0, 0)], Some(true)),
        (
            "running",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SRUN), size, 0),
            ],
            Some(false),
        ),
        (
            "stopped",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SSTOP), size, 0),
            ],
            Some(false),
        ),
        (
            "zombie",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SZOMB), size, 0),
            ],
            Some(true),
        ),
        (
            "gone",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SRUN), 0, libc::ESRCH),
            ],
            Some(true),
        ),
        (
            "moved-group",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 987, libc::SRUN), size, 0),
            ],
            None,
        ),
        ("permission", vec![list(64, &[], 0, libc::EPERM)], None),
        ("negative", vec![list(64, &[], -1, 0)], None),
        ("partial-pid", vec![list(64, &[101], 3, 0)], None),
        ("oversized", vec![list(64, &[], 260, 0)], None),
        ("duplicate", vec![list(64, &[101, 101], 8, 0)], None),
        ("zero-pid", vec![list(64, &[0], 4, 0)], None),
        ("negative-pid", vec![list(64, &[-1], 4, 0)], None),
        (
            "short-record",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SRUN), size - 1, 0),
            ],
            None,
        ),
        (
            "wrong-pid",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(102, 123, libc::SRUN), size, 0),
            ],
            None,
        ),
        (
            "unknown-state",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, 0), size, 0),
            ],
            None,
        ),
        (
            "denied-member",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SRUN), 0, libc::EACCES),
            ],
            None,
        ),
        (
            "unexplained-zero",
            vec![
                list(64, &[101], 4, 0),
                bsd(101, process(101, 123, libc::SRUN), 0, 0),
            ],
            None,
        ),
        ("stale-errno", vec![list(64, &[101], 4, libc::EPERM)], None),
    ];
    for (name, rows, expected) in cases {
        let guard = observe(rows.clone());
        let result = record_empty(&record());
        assert_eq!(
            result.ok(),
            expected,
            "macOS group uncertainty differs: {name}"
        );
        consumed();
        drop(guard);
        let _guard = observe(rows);
        let state = claims::state(&claims::Claim::Record(record()), "fixture-host");
        assert_eq!(
            state,
            match expected {
                Some(true) => claims::State::Free,
                Some(false) => claims::State::Busy,
                None => claims::State::Unknown,
            },
            "macOS claim uncertainty differs: {name}"
        );
        consumed();
    }
}

#[test]
fn macos_group_growth_is_bounded_and_never_calls_truncation_empty() {
    let full: Vec<i32> = (101..165).collect();
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    {
        let _guard = observe(vec![
            list(64, &full, 256, 0),
            list(0, &[], 512, 0),
            list(128, &[101], 4, 0),
            bsd(101, process(101, 123, libc::SZOMB), size, 0),
        ]);
        assert!(record_empty(&record()).expect("Complete zombie observation"));
        consumed();
    }
    for (name, rows) in [
        (
            "still-full",
            vec![
                list(64, &full, 256, 0),
                list(0, &[], 512, 0),
                list(128, &[], 512, 0),
            ],
        ),
        (
            "capacity-bound",
            vec![list(64, &full, 256, 0), list(0, &[], 262_148, 0)],
        ),
        (
            "size-permission",
            vec![list(64, &full, 256, 0), list(0, &[], 0, libc::EPERM)],
        ),
        (
            "partial-size",
            vec![list(64, &full, 256, 0), list(0, &[], 513, 0)],
        ),
    ] {
        let _guard = observe(rows);
        assert!(
            record_empty(&record()).is_err(),
            "Incomplete macOS group accepted: {name}"
        );
        consumed();
    }
}

#[test]
fn macos_public_identity_distinguishes_absence_from_observation_failure() {
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    for (returned, error, expected) in [
        (0, libc::ESRCH, Some(false)),
        (0, libc::EPERM, None),
        (0, libc::EACCES, None),
        (0, 0, None),
        (size - 1, 0, None),
        (size + 1, 0, None),
        (size, libc::EPERM, None),
        (size, 0, Some(true)),
    ] {
        let _guard = observe(vec![bsd(
            123,
            process(123, 123, libc::SRUN),
            returned,
            error,
        )]);
        assert_eq!(
            identity::alive(123, None).ok(),
            expected,
            "macOS process identity differs"
        );
        consumed();
    }
    let _guard = observe(vec![]);
    assert!(!identity::alive(0, None).expect("PID zero is absent"));
    assert_eq!(identity::started_at(0).expect("PID zero is absent"), None);
    assert_eq!(
        identity::started_at(u32::MAX)
            .expect_err("Invalid PID")
            .kind(),
        io::ErrorKind::InvalidInput
    );
    consumed();
}

#[test]
fn macos_birth_identity_refuses_malformed_records_and_reused_tokens() {
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    let valid = process(123, 123, libc::SRUN);
    let token = {
        let _guard = observe(vec![bsd(123, valid, size, 0)]);
        let token = identity::started_at(123)
            .expect("Complete process identity")
            .expect("Live PID");
        consumed();
        token
    };
    {
        let _guard = observe(vec![bsd(123, valid, size, 0)]);
        assert!(identity::alive(123, Some(token)).expect("Matching token"));
        consumed();
    }
    {
        let _guard = observe(vec![bsd(123, valid, size, 0)]);
        assert!(!identity::alive(123, Some(token + 1)).expect("Reused token"));
        consumed();
    }
    for field in ["pid", "state", "seconds", "microseconds"] {
        let mut damaged = valid;
        match field {
            "pid" => damaged.pbi_pid = 124,
            "state" => damaged.pbi_status = 6,
            "seconds" => damaged.pbi_start_tvsec = u64::MAX,
            "microseconds" => damaged.pbi_start_tvusec = 1_000_000,
            _ => unreachable!(),
        }
        let _guard = observe(vec![bsd(123, damaged, size, 0)]);
        assert!(
            identity::started_at(123).is_err(),
            "Malformed macOS identity accepted: {field}"
        );
        consumed();
    }
}
