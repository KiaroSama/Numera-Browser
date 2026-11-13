#![cfg(unix)]
use super::*;
use numera_core::ownership::observation::{
    self, Enumeration, GroupSource, Row, SignalIdentity, SignalReason, SignalSource,
};
#[derive(Default)]
struct Rows {
    rows: Vec<Row>,
    complete: bool,
    errno: Option<i32>,
    timeout: bool,
    private_error: bool,
    enumerations: usize,
    reads: Vec<u32>,
    forbid_reads: bool,
}
impl Rows {
    fn complete(rows: Vec<Row>) -> Self {
        Self {
            rows,
            complete: true,
            ..Self::default()
        }
    }
}
impl GroupSource for Rows {
    fn enumerate(&mut self, _group: i32) -> io::Result<Enumeration> {
        self.enumerations += 1;
        if self.timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "private fixture query text",
            ));
        }
        if self.private_error {
            return Err(io::Error::other("private fixture text: secret data"));
        }
        if let Some(errno) = self.errno {
            return Err(io::Error::from_raw_os_error(errno));
        }
        Ok(Enumeration {
            pids: self.rows.iter().map(|row| row.pid).collect(),
            complete: self.complete,
            filtered: false,
        })
    }
    fn process(&mut self, pid: u32) -> io::Result<Option<Row>> {
        assert!(!self.forbid_reads, "ignored pid was inspected");
        self.reads.push(pid);
        Ok(self.rows.iter().find(|row| row.pid == pid).cloned())
    }
}
fn row(pid: u32, group: i32, state: &[u8]) -> Row {
    Row {
        pid,
        group,
        state_codes: state.to_vec(),
    }
}
#[derive(Default)]
struct Signals {
    mismatch: bool,
    caller: bool,
    errno: Option<i32>,
    births: Vec<u32>,
    signals: Vec<(i32, bool)>,
}
impl SignalSource for Signals {
    fn birth(&mut self, pid: u32) -> io::Result<Option<u64>> {
        self.births.push(pid);
        Ok(Some(if self.mismatch { 8 } else { 7 }))
    }
    fn caller_group(&mut self) -> i32 {
        if self.caller { 123456789 } else { 1 }
    }
    fn deliver(&mut self, target: i32, force: bool) -> io::Result<()> {
        self.signals.push((target, force));
        if let Some(errno) = self.errno {
            Err(io::Error::from_raw_os_error(errno))
        } else {
            Ok(())
        }
    }
}
source_test!(
    posix_observation_errors_ps_failure_diagnostic_is_bounded_numeric_and_does_not_query_again,
    "tests/test_posix_observation_errors.py::test_ps_failure_diagnostic_is_bounded_numeric_and_does_not_query_again",
    {
        let mut source = Rows::complete(vec![row(456, 123, b"S")]);
        assert!(!observation::observe_group(123, None, &mut source)?);
        assert_eq!(source.enumerations, 1);
        assert_eq!(source.reads, vec![456]);
        let output = observation::diagnostic(123, true, false, true);
        assert_eq!(source.enumerations, 1);
        assert_eq!(source.reads, vec![456]);
        for field in [
            "group=123",
            "pid=456",
            "state_codes=(83,)",
            "graceful_signal=true force_signal=false",
        ] {
            assert!(
                output.contains(field),
                "missing numeric observation field: {field}"
            );
        }
        assert!(output.len() < 512);
        assert!(!output.contains("private fixture"));
        Ok(())
    }
);
source_test!(
    posix_observation_errors_ps_failed_query_diagnostic_never_emits_raw_output,
    "tests/test_posix_observation_errors.py::test_ps_failed_query_diagnostic_never_emits_raw_output",
    {
        let mut source = Rows {
            errno: Some(1),
            ..Rows::default()
        };
        assert!(observation::observe_group(123, None, &mut source).is_err());
        let output = observation::diagnostic(123, false, false, true);
        assert!(output.contains("query_errno=1"));
        assert!(!output.contains("state_codes="));
        assert_eq!(source.enumerations, 1);
        assert!(source.reads.is_empty());
        for forbidden in ["private fixture", "secret data"] {
            assert!(!output.contains(forbidden));
        }
        let mut private = Rows {
            private_error: true,
            ..Rows::default()
        };
        let error = observation::observe_group(123, None, &mut private)
            .err()
            .ok_or("failed query proved emptiness")?;
        assert!(!error.to_string().contains("private fixture"));
        let output = observation::diagnostic(123, false, false, true);
        assert!(output.contains("query_failed=true"));
        assert!(!output.contains("secret data"));
        assert_eq!(private.enumerations, 1);
        Ok(())
    }
);
source_test!(posix_observation_errors_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries_0a7286e8,"tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[identity-ownership_mismatch]",{
let mut source=Signals{mismatch:true,caller:false,errno:None,..Signals::default()};let identity=SignalIdentity{pid:123456789,token:Some(7),group:Some(123456789)};let result=observation::attempt_signal(identity,true,&mut source);assert!(!result.delivered);assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),0);let output=observation::diagnostic(123456789,true,false,true);assert!(output.contains("ownership_mismatch"));assert!(output.contains("force=true"));assert!(!output.contains("private fixture"));assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),0);
Ok(())
});
source_test!(posix_observation_errors_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries_52f37eea,"tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[own-group-caller_group]",{
let mut source=Signals{mismatch:false,caller:true,errno:None,..Signals::default()};let identity=SignalIdentity{pid:123456789,token:Some(7),group:Some(123456789)};let result=observation::attempt_signal(identity,true,&mut source);assert!(!result.delivered);assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),0);let output=observation::diagnostic(123456789,true,false,true);assert!(output.contains("caller_group"));assert!(output.contains("force=true"));assert!(!output.contains("private fixture"));assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),0);
Ok(())
});
source_test!(posix_observation_errors_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries_b20b1bfb,"tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[3-signal_errno=3]",{
let mut source=Signals{mismatch:false,caller:false,errno:Some(3),..Signals::default()};let identity=SignalIdentity{pid:123456789,token:Some(7),group:Some(123456789)};let result=observation::attempt_signal(identity,true,&mut source);assert!(!result.delivered);assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),1);let output=observation::diagnostic(123456789,true,false,true);assert!(output.contains("signal_errno=3"));assert!(output.contains("force=true"));assert!(!output.contains("private fixture"));assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),1);
Ok(())
});
source_test!(posix_observation_errors_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries_5b246c5e,"tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[1-signal_errno=1]",{
let mut source=Signals{mismatch:false,caller:false,errno:Some(1),..Signals::default()};let identity=SignalIdentity{pid:123456789,token:Some(7),group:Some(123456789)};let result=observation::attempt_signal(identity,true,&mut source);assert!(!result.delivered);assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),1);let output=observation::diagnostic(123456789,true,false,true);assert!(output.contains("signal_errno=1"));assert!(output.contains("force=true"));assert!(!output.contains("private fixture"));assert_eq!(source.births,vec![identity.pid]);assert_eq!(source.signals.len(),1);
Ok(())
});
source_test!(
    posix_observation_errors_delivered_group_signal_is_recorded_without_changing_ownership,
    "tests/test_posix_observation_errors.py::test_delivered_group_signal_is_recorded_without_changing_ownership",
    {
        let identity = SignalIdentity {
            pid: 123456789,
            token: Some(7),
            group: Some(123456789),
        };
        let mut source = Signals::default();
        let result = observation::attempt_signal(identity, true, &mut source);
        assert!(result.delivered);
        assert_eq!(result.reason, SignalReason::Delivered);
        let output = observation::diagnostic(123456789, true, true, true);
        assert!(output.contains("reason=delivered"));
        assert_eq!(source.births, vec![123456789]);
        assert_eq!(source.signals, vec![(-123456789, true)]);
        assert_eq!(identity.pid, 123456789);
        assert_eq!(identity.token, Some(7));
        Ok(())
    }
);
source_test!(
    posix_observation_errors_healthy_running_and_reaped_child_controls,
    "tests/test_posix_observation_errors.py::test_healthy_running_and_reaped_child_controls",
    {
        assert!(observation::procfs_row(123, "123 (fixture) S 1 0").is_err());
        assert!(observation::procfs_process(123, "123 (fixture) S 1 0 0 0 0 0").is_err());
        assert!(observation::procfs_process(2, "2 (kernel) I 0 0 0 0 0 2097152")?.is_none());
        let mut invalid_group = Rows::complete(vec![row(123, 0, b"S")]);
        assert!(observation::observe_group(123, None, &mut invalid_group).is_err());
        let t = temp("native-observation-real-child")?;
        let mut child = command("leaf")?;
        child
            .arg(t.path().join("pid"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        wait_file(&t.path().join("pid"))?;
        let record = as_record(&tree);
        assert!(process_running(tree.pid(), record["pidToken"].as_u64())?);
        assert!(!tree.empty()?);
        tree.settle(Duration::from_secs(3))?;
        assert!(tree.empty()?);
        assert!(!process_running(tree.pid(), record["pidToken"].as_u64())?);
        Ok(())
    }
);
#[cfg(target_os = "linux")]
source_test!(
    posix_observation_errors_end_tree_cannot_report_a_live_child_stopped_from_failed_observation,
    "tests/test_posix_observation_errors.py::test_end_tree_cannot_report_a_live_child_stopped_from_failed_observation",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Denied {
            target: PathBuf,
        }
        impl Backend for Denied {
            fn stat(&self, path: &Path) -> Option<io::Result<String>> {
                (path == self.target).then(|| Err(io::Error::from_raw_os_error(13)))
            }
        }
        let t = temp("native-observation-stop-refusal")?;
        let mut child = command("leaf")?;
        child
            .arg(t.path().join("pid"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        wait_file(&t.path().join("pid"))?;
        let target = PathBuf::from(format!("/proc/{}/stat", tree.pid()));
        let stopped = with_backend(Arc::new(Denied { target }), || {
            tree.settle(Duration::from_millis(40))
        });
        let leader_exited = tree.child_mut().try_wait()?.is_some();
        assert!(
            stopped.is_err() || leader_exited,
            "incomplete table falsely proved live child stopped"
        );
        tree.settle(Duration::from_secs(3))?;
        assert!(tree.empty()?);
        Ok(())
    }
);
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_808136c7,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z 123\\n987 S 987\\n-False-1]",{let expected_running=false;
let mut source=Rows{rows:vec![row(123,123,b"Z"),row(987,987,b"S")],complete:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_3f936a0a,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z+ 123\\n123 Z 456\\n-False-1]",{let expected_running=false;
let mut source=Rows{rows:vec![row(123,123,b"Z+"),row(456,123,b"Z")],complete:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_623ca5ae,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z 123\\n123 S 456\\n-True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![row(123,123,b"Z"),row(456,123,b"S")],complete:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_8d266e4e,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 R<s 123\\n-True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![row(123,123,b"R<s")],complete:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_c67da92a,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-1-987 S 987\\n-True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![row(987,987,b"S")],complete:false,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.is_empty());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_c0761779,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-malformed\\n-True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![],complete:false,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_e4cb6571,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 ? 123\\n-True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![row(123,123,b"?")],complete:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_4030b0f3,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0--True-1]",{let expected_running=true;
let mut source=Rows{rows:vec![],complete:false,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.len()<=source.rows.len());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_5aea7f18,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-None--True-1]",{let expected_running=true;
let mut source=Rows{timeout:true,..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.is_empty());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_13d23ef4,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-5-0-123 Z 123\\n-True-0]",{let expected_running=true;
let mut source=Rows{errno:Some(5),..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.is_empty());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_694691a7,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[linux-1-0-123 Z 123\\n-True-0]",{let expected_running=true;
let mut source=Rows{errno:Some(1),..Rows::default()};let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.is_empty());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(posix_observation_errors_darwin_denied_probe_requires_complete_zombie_aware_observation_81991b21,"tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-3-0-123 S 123\\n-False-0]",{let expected_running=false;
let mut source=Rows::complete(vec![]);let observed=observation::observe_group(123,None,&mut source);assert_eq!(!matches!(observed,Ok(true)),expected_running);assert_eq!(source.enumerations,1);assert!(source.reads.is_empty());let count=source.reads.len();let output=observation::diagnostic(123,true,false,true);assert_eq!(source.enumerations,1);assert_eq!(source.reads.len(),count);assert!(!output.contains("private fixture"));
Ok(())
});
source_test!(
    posix_observation_errors_failed_probe_invalidates_a_previous_live_process_sample,
    "tests/test_posix_observation_errors.py::test_failed_probe_invalidates_a_previous_live_process_sample",
    {
        let mut live = Rows::complete(vec![row(123, 123, b"R<s")]);
        assert!(!observation::observe_group(123, None, &mut live)?);
        assert!(observation::diagnostic(123, true, false, true).contains("state_codes="));
        let mut failed = Rows {
            errno: Some(5),
            ..Rows::default()
        };
        assert!(observation::observe_group(123, None, &mut failed).is_err());
        let output = observation::diagnostic(123, true, false, true);
        assert!(output.contains("query_errno=5"));
        assert!(!output.contains("state_codes="));
        assert!(!output.contains("private fixture"));
        assert_eq!(failed.enumerations, 1);
        assert!(failed.reads.is_empty());
        Ok(())
    }
);
#[cfg(target_os = "linux")]
source_test!(
    posix_observation_errors_ignored_pid_is_excluded_before_its_stat_is_read,
    "tests/test_posix_observation_errors.py::test_ignored_pid_is_excluded_before_its_stat_is_read",
    {
        let mut source = Rows {
            rows: vec![row(123456789, 123456789, b"S")],
            complete: true,
            forbid_reads: true,
            ..Rows::default()
        };
        assert!(observation::observe_group(
            123456789,
            Some(123456789),
            &mut source
        )?);
        assert_eq!(source.enumerations, 1);
        assert!(source.reads.is_empty());
        Ok(())
    }
);
