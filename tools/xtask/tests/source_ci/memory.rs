use super::*;
use numera_core::ownership::memory::{self as native, Backend, JointSnapshot, Row, Snapshot};
use std::cell::Cell;
fn stat(pid: u32, group: u32, birth: u64) -> Vec<u8> {
    let mut fields = vec!["0".to_owned(); 20];
    fields[0] = "S".into();
    fields[1] = "1".into();
    fields[2] = group.to_string();
    fields[19] = birth.to_string();
    format!(
        "{pid} (private command (not a label)) {}\n",
        fields.join(" ")
    )
    .into_bytes()
}
fn process(temp: &Temp, pid: u32, group: u32, rss: u64, peak: u64) -> TestResult {
    temp.write(&format!("{pid}/stat"), &stat(pid, group, 900))?;
    temp.write(
        &format!("{pid}/status"),
        format!("Name:\tPRIVATE\nVmRSS:\t{rss} kB\nVmHWM:\t{peak} kB\n").as_bytes(),
    )?;
    Ok(())
}
struct Reuse {
    reads: usize,
    replace: bool,
}
impl memory::Procfs for Reuse {
    fn entries(&mut self, root: &Path) -> io::Result<Vec<PathBuf>> {
        memory::Procfs::entries(&mut memory::NativeProcfs, root)
    }
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        let bytes = memory::Procfs::read(&mut memory::NativeProcfs, path)?;
        if path.ends_with("41/stat") {
            self.reads += 1;
            if self.replace && self.reads >= 2 {
                return Ok(stat(41, 99, 901));
            }
        }
        Ok(bytes)
    }
}
fn linux(damage: &str) -> TestResult {
    let temp = Temp::new("procfs")?;
    process(&temp, 41, 41, 13, 23)?;
    process(&temp, 42, 42, 17, 29)?;
    process(&temp, 43, 42, 31, 47)?;
    process(&temp, 99, 99, 1, 1)?;
    temp.write("99/status", b"must never be parsed")?;
    match damage {
        "missing" => fs::remove_file(temp.0.join("41/status"))?,
        "partial" => {
            temp.write("41/status", b"VmRSS: 1 kB\n")?;
        }
        "negative" => {
            temp.write("41/status", b"VmRSS: -1 kB\nVmHWM: 2 kB\n")?;
        }
        "duplicate" => {
            temp.write("41/status", b"VmRSS: 1 kB\nVmRSS: 1 kB\nVmHWM: 2 kB\n")?;
        }
        _ => (),
    }
    let value = memory::linux_snapshot(
        &mut Reuse {
            reads: 0,
            replace: damage == "reuse",
        },
        &temp.0,
        &BTreeMap::from([(41, 900), (42, 900)]),
        if damage == "limit" { 1 } else { 512 },
    )?;
    if damage == "None" {
        assert_eq!(value.missing, 0);
        assert_eq!(
            value
                .rows
                .iter()
                .map(|row| (
                    row.pid,
                    row.birth,
                    row.counters["rss_bytes"],
                    row.counters["peak_rss_bytes"]
                ))
                .collect::<Vec<_>>(),
            vec![
                (41, 900, 13 * 1024, 23 * 1024),
                (42, 900, 17 * 1024, 29 * 1024),
                (43, 900, 31 * 1024, 47 * 1024)
            ]
        );
    } else {
        assert!(value.missing > 0);
        assert!(damage == "limit" || value.rows.iter().all(|row| row.pid != 41));
    }
    assert!(value.rows.iter().all(|row| row.pid != 99));
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_linux_memory_reads_only_owned_groups_and_rejects_partial_evidence",linux,[
 memory_linux_good=>("None","None"),memory_linux_missing=>("missing","missing"),memory_linux_partial=>("partial","partial"),memory_linux_negative=>("negative","negative"),memory_linux_duplicate=>("duplicate","duplicate"),memory_linux_reuse=>("reuse","reuse"),memory_linux_limit=>("limit","limit")
]);
fn owner(pid: u32, birth: u64) -> memory::Owner {
    memory::Owner {
        pid,
        birth,
        group: Some(pid),
    }
}
fn row(pid: u32, birth: u64, rss: u64, peak: u64) -> Row {
    Row {
        pid,
        birth,
        counters: BTreeMap::from([("rss_bytes".into(), rss), ("peak_rss_bytes".into(), peak)]),
    }
}
fn recorder(damage: &str) -> TestResult {
    let direct = owner(41, 900);
    let sdk = owner(42, 901);
    let prior = owner(10, 800);
    let mut recorder =
        memory::Recorder::new(direct.clone(), BTreeSet::from([prior.clone()]), false);
    if damage == "capacity" {
        recorder.max_snapshots = 1;
    }
    let calls = Cell::new(0);
    for now in [0.0, 0.0, 0.1] {
        recorder.sample(
            now,
            damage != "receipt",
            &[prior.clone(), direct.clone(), sdk.clone()],
            |groups| {
                calls.set(calls.get() + 1);
                assert_eq!(*groups, BTreeMap::from([(41, 900), (42, 901)]));
                if damage == "query" {
                    return Err(io::Error::other(
                        "private path and credentials must not be exposed",
                    )
                    .into());
                }
                Ok((
                    Snapshot {
                        rows: vec![
                            row(
                                41,
                                if damage == "reused-command" { 999 } else { 900 },
                                11,
                                21,
                            ),
                            row(42, if damage == "reused-sdk" { 999 } else { 901 }, 31, 41),
                            row(43, 902, 71, 91),
                            row(44, 903, 61, 81),
                        ],
                        missing: 0,
                    },
                    None,
                ))
            },
        )?;
    }
    let value = recorder.result();
    assert_eq!(value["coverage"], "sampled-not-lifetime-attribution");
    assert_eq!(value.as_object().expect("object").len(), 5);
    if ["receipt", "query"].contains(&damage) {
        assert!(value["missing"].as_u64().expect("count") > 0);
        assert!(value["roles"].as_object().expect("roles").is_empty());
    } else {
        assert!(calls.get() > 0);
        if damage == "reused-command" {
            assert!(value["roles"].get("command").is_none());
        } else {
            assert_eq!(
                value["roles"]["command"]["max_observed"]["peak_rss_bytes"],
                21
            );
        }
        if damage == "reused-sdk" {
            assert!(value["roles"].get("sdk-leader").is_none());
        } else {
            assert_eq!(
                value["roles"]["sdk-leader"]["max_observed"]["peak_rss_bytes"],
                41
            );
        }
        assert_eq!(
            value["roles"]["owned-descendant"]["identities"],
            if damage.starts_with("reused-") { 3 } else { 2 }
        );
        assert_eq!(
            value["roles"]["owned-descendant"]["max_observed"]["rss_bytes"],
            71
        );
        assert_eq!(
            value["roles"]["owned-descendant"]["max_observed"]["peak_rss_bytes"],
            91
        );
        assert_eq!(value["snapshots"], if damage == "capacity" { 1 } else { 2 });
        assert_eq!(value["truncated"], damage == "capacity");
    }
    let text = value.to_string();
    assert!(!text.contains("private path") && !text.contains("credentials"));
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_recorder_preserves_roles_maxima_and_sampled_coverage_without_summing",recorder,[
 memory_recorder_good=>("None","None"),memory_recorder_receipt=>("receipt","receipt"),memory_recorder_query=>("query","query"),memory_recorder_capacity=>("capacity","capacity"),memory_recorder_sdk=>("reused-sdk","reused-sdk"),memory_recorder_command=>("reused-command","reused-command")
]);
fn real_procfs(damage: &str) -> TestResult {
    let temp = Temp::new("record-procfs")?;
    process(&temp, 41, 41, 13, 23)?;
    process(&temp, 42, 42, 17, 29)?;
    process(&temp, 43, 42, 31, 47)?;
    let direct = owner(41, 900);
    let sdk = owner(42, if damage == "reused-leader" { 899 } else { 900 });
    let mut recorder = memory::Recorder::new(direct.clone(), BTreeSet::new(), false);
    recorder.sample(1.0, true, &[direct, sdk], |groups| {
        memory::linux_snapshot(&mut memory::NativeProcfs, &temp.0, groups, 512)
            .map(|value| (value, None))
    })?;
    let value = recorder.result();
    assert_eq!(
        value["roles"]["command"]["max_observed"]["rss_bytes"],
        13 * 1024
    );
    if damage == "reused-leader" {
        assert!(value["missing"].as_u64().expect("missing") > 0);
        assert_eq!(value["roles"].as_object().expect("roles").len(), 1);
    } else {
        assert_eq!(value["missing"], 0);
        assert_eq!(
            value["roles"]["sdk-leader"]["max_observed"]["peak_rss_bytes"],
            29 * 1024
        );
        assert_eq!(
            value["roles"]["owned-descendant"]["max_observed"]["peak_rss_bytes"],
            47 * 1024
        );
    }
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_recorder_reaches_real_procfs_membership_and_counter_parser",real_procfs,[memory_real_good=>("None","None"),memory_real_reused=>("reused-leader","reused-leader")]);
fn projection(damage: &str) -> TestResult {
    let mut value = json!({"coverage":"sampled-not-lifetime-attribution","snapshots":2,"missing":1,"truncated":false,"roles":{"command":{"identities":1,"max_observed":{"rss_bytes":123}}}});
    match damage {
        "extra" => value["private"] = json!("do-not-publish"),
        "role" => {
            let row = value["roles"]
                .as_object_mut()
                .expect("object")
                .remove("command")
                .expect("row");
            value["roles"]["do-not-publish"] = row;
        }
        "counter" => value["roles"]["command"]["max_observed"]["private-path"] = json!(1),
        "number" => {
            value["roles"]["command"]["max_observed"]["rss_bytes"] = json!("do-not-publish")
        }
        "coverage" => value["coverage"] = json!("lifetime-peak"),
        _ => (),
    }
    let events = memory::project(&value, "native");
    if damage == "None" {
        let events = events?;
        assert_eq!(events.len(), 2);
        assert!(events[1].contains(
            "runtime=native role=command identities=1 metric=rss_bytes max_observed=123"
        ));
        assert!(events[0].contains("sampled-not-lifetime-attribution"));
    } else {
        assert!(events.is_err());
    }
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_diagnostic_projection_never_forwards_unknown_text",projection,[memory_project_good=>("None","None"),memory_project_extra=>("extra","extra"),memory_project_role=>("role","role"),memory_project_counter=>("counter","counter"),memory_project_number=>("number","number"),memory_project_coverage=>("coverage","coverage")]);

struct Windows {
    failed_close: bool,
    calls: Vec<String>,
    members: usize,
    peak: usize,
    tick: usize,
    damage: &'static str,
}
impl Windows {
    fn new(damage: &'static str) -> Self {
        Self {
            failed_close: false,
            calls: Vec::new(),
            members: 0,
            peak: 0,
            tick: 0,
            damage,
        }
    }
}
impl Backend for Windows {
    type Process = u32;
    fn members(&mut self) -> io::Result<Vec<u32>> {
        self.calls.push("members".into());
        self.members += 1;
        Ok(if self.damage == "membership" && self.members == 3 {
            vec![41, 43]
        } else if self.damage == "handles" {
            vec![41, 42, 43, 99]
        } else {
            vec![41, 42]
        })
    }
    fn open(&mut self, pid: u32) -> io::Result<Option<u32>> {
        self.calls.push(format!("open:{pid}"));
        Ok(Some(pid + 100))
    }
    fn observe(&mut self, process: &u32, pid: u32) -> io::Result<Option<Row>> {
        self.calls.push(format!("observe:{pid}"));
        if self.damage == "query" {
            return Err(io::Error::other("private query failure"));
        }
        if [143, 199].contains(process) {
            return Ok(None);
        }
        Ok(Some(Row {
            pid,
            birth: u64::from(*process),
            counters: BTreeMap::from([
                ("rss_bytes".into(), 11),
                ("peak_rss_bytes".into(), 17),
                ("private_bytes".into(), 23),
                ("peak_private_bytes".into(), 29),
            ]),
        }))
    }
    fn close(&mut self, process: u32) -> io::Result<()> {
        self.calls.push(format!("close:{process}"));
        if self.failed_close {
            Err(io::Error::other(
                "Owned memory diagnostic process handle did not close",
            ))
        } else {
            Ok(())
        }
    }
    fn peak(&mut self) -> io::Result<u64> {
        self.calls.push("peak".into());
        self.peak += 1;
        Ok(if self.peak == 1 {
            1000
        } else if self.damage == "peak" {
            900
        } else {
            1100
        })
    }
    fn ticks(&mut self) -> io::Result<u64> {
        self.tick += 1;
        Ok(if self.tick == 1 {
            100
        } else if self.damage == "clock" {
            90
        } else {
            110
        })
    }
    fn snapshot(&mut self) -> io::Result<Snapshot> {
        self.calls.push("snapshot".into());
        if self.damage == "query" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private query failure",
            ));
        }
        native::observe(self)
    }
}
fn handles(failed: bool) -> TestResult {
    let mut backend = Windows::new("handles");
    backend.failed_close = failed;
    let result = native::observe(&mut backend);
    if failed {
        assert!(result.is_err());
        assert_eq!(
            backend.calls,
            vec!["members", "open:41", "observe:41", "close:141"]
        );
    } else {
        let result = result?;
        assert_eq!(result.missing, 2);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| (row.pid, row.birth))
                .collect::<Vec<_>>(),
            vec![(41, 141), (42, 142)]
        );
        assert_eq!(
            backend
                .calls
                .iter()
                .filter(|line| line.starts_with("close:"))
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["close:141", "close:142", "close:143", "close:199"]
        );
    }
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_windows_memory_uses_retained_job_and_closes_process_handles",handles,[memory_handles_good=>("False",false),memory_handles_failed=>("True",true)]);
fn joint(damage: &'static str) -> TestResult {
    let mut backend = Windows::new(damage);
    let result = native::observe_joint(&mut backend);
    if ["peak", "clock", "query"].contains(&damage) {
        assert!(result.is_err());
    } else {
        let value = result?;
        assert_eq!(value.started_ns, 100);
        assert_eq!(value.finished_ns, 110);
        assert_eq!(value.peak_before_bytes, 1000);
        assert_eq!(value.peak_after_bytes, 1100);
        assert_eq!(value.membership_changed, damage == "membership");
        assert_eq!(value.snapshot.missing, u64::from(damage == "membership"));
    }
    assert_eq!(&backend.calls[..3], &["members", "peak", "snapshot"]);
    if damage != "query" {
        assert!(backend.calls.iter().any(|call| call == "close:141"));
    }
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_joint_windows_snapshot_brackets_the_same_current_observation",joint,[memory_joint_good=>("None","None"),memory_joint_members=>("membership","membership"),memory_joint_peak=>("peak","peak"),memory_joint_clock=>("clock","clock"),memory_joint_query=>("query","query")]);
fn windows_recorder(damage: &str) -> TestResult {
    let direct = owner(41, 900);
    let sdk = owner(42, 901);
    let mut recorder = memory::Recorder::new(direct.clone(), BTreeSet::new(), true);
    if damage == "capacity" {
        recorder.max_intervals = 1;
    }
    let calls = Cell::new(0u64);
    for now in [1.0, 1.0, 1.1] {
        recorder.sample(now, true, &[direct.clone(), sdk.clone()], |_| {
            let index = calls.get() + 1;
            calls.set(index);
            if damage == "query" {
                return Err(io::Error::other("private diagnostic must not be published").into());
            }
            let rows = [
                (41, 900, 10 * index, 900),
                (42, 901, 20, 800),
                (43, 902, 30, 700),
                (44, 903, 40, 600),
            ]
            .into_iter()
            .map(|(pid, birth, private, peak)| Row {
                pid,
                birth,
                counters: BTreeMap::from([
                    ("private_bytes".into(), private),
                    ("peak_private_bytes".into(), peak),
                ]),
            })
            .collect();
            let snapshot = Snapshot {
                rows,
                missing: u64::from(damage == "changed-members"),
            };
            let joint = JointSnapshot {
                snapshot: snapshot.clone(),
                started_ns: index * 100,
                finished_ns: index * 100 + 10,
                peak_before_bytes: index * 1000,
                peak_after_bytes: index * 1000 + 100,
                membership_changed: damage == "changed-members",
            };
            Ok((snapshot, Some(joint)))
        })?;
    }
    let value = recorder.result();
    assert_eq!(calls.get(), 2);
    if damage == "query" {
        assert_eq!(value["missing"], 2);
        assert_eq!(value["windows_intervals"], json!([]));
    } else {
        let intervals = value["windows_intervals"].as_array().expect("intervals");
        assert_eq!(intervals.len(), if damage == "capacity" { 1 } else { 2 });
        assert_eq!(
            intervals[0],
            json!({"started_ns":100,"finished_ns":110,"peak_before_bytes":1000,"peak_after_bytes":1100,"membership_changed":damage=="changed-members","missing":u64::from(damage=="changed-members"),"private_bytes":{"command":10,"sdk-leader":20,"owned-descendant":70},"members":{"command":1,"sdk-leader":1,"owned-descendant":2}})
        );
        if damage != "capacity" {
            assert_eq!(intervals[1]["private_bytes"]["command"], 20);
        } else {
            assert_eq!(value["truncated"], true);
        }
        assert_eq!(
            value["roles"]["owned-descendant"]["max_observed"]["peak_private_bytes"],
            700
        );
        let events = memory::project(&value, "native")?;
        assert!(events.iter().any(|event| event.contains("interval=0")
            && event.contains("owned_descendant_private_bytes=70")));
    }
    assert!(!value.to_string().contains("private diagnostic"));
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_windows_recorder_retains_joint_current_private_vectors",windows_recorder,[memory_windows_good=>("None","None"),memory_windows_query=>("query","query"),memory_windows_members=>("changed-members","changed-members"),memory_windows_capacity=>("capacity","capacity")]);
fn interval_projection(damage: &str) -> TestResult {
    let interval = json!({"started_ns":100,"finished_ns":110,"peak_before_bytes":1000,"peak_after_bytes":1100,"membership_changed":false,"missing":0,"private_bytes":{"command":10,"sdk-leader":10,"owned-descendant":10},"members":{"command":1,"sdk-leader":1,"owned-descendant":1}});
    let mut value = json!({"coverage":"sampled-not-lifetime-attribution","snapshots":1,"missing":0,"truncated":false,"roles":{},"windows_intervals":[interval.clone()]});
    match damage {
        "text" => value["windows_intervals"][0]["private"] = json!("do-not-publish"),
        "counter" => {
            value["windows_intervals"][0]["private_bytes"]["command"] = json!("do-not-publish")
        }
        "member" => value["windows_intervals"][0]["members"]["command"] = json!(513),
        "peak" => value["windows_intervals"][0]["peak_after_bytes"] = json!(999),
        "time" => value["windows_intervals"][0]["finished_ns"] = json!(99),
        _ => value["windows_intervals"] = json!(vec![interval; 129]),
    }
    assert!(memory::project(&value, "native").is_err());
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_joint_memory_projection_rejects_invalid_intervals_before_logging",interval_projection,[memory_interval_text=>("text","text"),memory_interval_counter=>("counter","counter"),memory_interval_member=>("member","member"),memory_interval_peak=>("peak","peak"),memory_interval_time=>("time","time"),memory_interval_limit=>("limit","limit")]);
fn members(damage: &str) -> TestResult {
    for width in [4, 8] {
        let mut raw = Vec::new();
        raw.extend_from_slice(&(if damage == "truncated" { 3u32 } else { 2 }).to_le_bytes());
        raw.extend_from_slice(&2u32.to_le_bytes());
        for pid in [
            41u64,
            if damage == "duplicate" {
                41
            } else if damage == "zero" {
                0
            } else {
                42
            },
        ] {
            raw.extend_from_slice(&pid.to_le_bytes()[..width]);
        }
        let result =
            native::members_from_buffer(&raw, if damage == "short" { 4 } else { raw.len() }, width);
        if damage == "None" {
            assert_eq!(result?, vec![41, 42]);
        } else {
            assert!(result.is_err());
        }
    }
    Ok(())
}
source_cases!("tests/test_browser_memory_diagnostics.py::test_windows_job_membership_refuses_incomplete_or_invalid_lists",members,[memory_members_good=>("None","None"),memory_members_short=>("short","short"),memory_members_truncated=>("truncated","truncated"),memory_members_duplicate=>("duplicate","duplicate"),memory_members_zero=>("zero","zero")]);
