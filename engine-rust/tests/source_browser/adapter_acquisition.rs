use super::*;
use std::{
    fs,
    io::{self, BufReader, Cursor, Read, Write},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
struct Worker {
    tree: numera_core::ownership::tree::OwnedTree,
    reader: BufReader<std::process::ChildStdout>,
    input: std::process::ChildStdin,
    trace: std::path::PathBuf,
}
impl Worker {
    fn new(
        root: &std::path::Path,
        mode: &str,
        selected: Option<&str>,
        mutation: Option<&str>,
    ) -> Self {
        let driver = support::vendor(&root.join("driver"), mode);
        let mut c = support::worker(root);
        c.env("NUMERA_PLAYWRIGHT_DRIVER", &driver)
            .env("NUMERA_CLOAK_DRIVER", &driver)
            .env("CLOAKBROWSER_AUTO_UPDATE", "true")
            .env("NUMERA_TEST_SDK_TRACE", root.join("sdk-trace.jsonl"))
            .env("NUMERA_TEST_SDK_SCHEDULING", mutation.unwrap_or(""));
        if let Some(selected) = selected {
            c.env("CLOAKBROWSER_BINARY_PATH", selected);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut tree =
            numera_core::ownership::tree::OwnedTree::spawn(&mut c).expect("browser contract");
        let reader = BufReader::new(tree.child_mut().stdout.take().expect("browser contract"));
        let input = tree.child_mut().stdin.take().expect("browser contract");
        Self {
            tree,
            reader,
            input,
            trace: root.join("sdk-trace.jsonl"),
        }
    }
    fn call(&mut self, v: Value, fragments: usize) -> Value {
        let mut bytes = serde_json::to_vec(&v).expect("browser contract");
        bytes.push(b'\n');
        let size = bytes.len().div_ceil(fragments + 1);
        let mut reads = trace(&self.trace)
            .iter()
            .filter(|row| row["event"] == "read")
            .count();
        let chunks = bytes.chunks(size).collect::<Vec<_>>();
        for (index, chunk) in chunks.iter().enumerate() {
            self.input.write_all(chunk).unwrap_or_else(|error| {
                panic!(
                    "native worker write: {error}; operation={}; trace={:?}; log={}",
                    v["operation"],
                    trace(&self.trace),
                    support::logs(self.trace.parent().expect("worker root"))
                )
            });
            self.input.flush().expect("browser contract");
            if fragments > 0 && index + 1 < chunks.len() {
                reads += 1;
                let end = Instant::now() + Duration::from_secs(5);
                while trace(&self.trace)
                    .iter()
                    .filter(|row| row["event"] == "read")
                    .count()
                    < reads
                {
                    assert!(
                        Instant::now() < end,
                        "fragment was not consumed by native input"
                    );
                    std::thread::park_timeout(Duration::from_millis(1));
                }
            }
        }
        browser::adapter::frame(&mut self.reader)
            .expect("browser contract")
            .unwrap_or_else(|| {
                panic!(
                    "native worker EOF; operation={}; trace={:?}; log={}",
                    v["operation"],
                    trace(&self.trace),
                    support::logs(self.trace.parent().expect("worker root"))
                )
            })
    }
    fn hello(&mut self) {
        assert_eq!(
            self.call(json!({"id":1,"protocol":1,"operation":"hello"}), 0)["ok"],
            true
        )
    }
    fn finish(mut self) -> i32 {
        drop(self.input);
        let end = Instant::now() + Duration::from_secs(5);
        let code = loop {
            if let Some(status) = self.tree.child_mut().try_wait().expect("browser contract") {
                break status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < end, "native worker failed to exit");
            std::thread::park_timeout(Duration::from_millis(2));
        };
        self.tree
            .settle(Duration::from_secs(3))
            .expect("browser contract");
        code
    }
}
fn missing(selection: &str) {
    let d = fresh();
    let selected = match selection {
        "absent" => None,
        "empty" => Some(String::new()),
        "missing" => Some(
            d.path()
                .join("not-there")
                .to_str()
                .expect("browser contract")
                .into(),
        ),
        "directory" => Some(d.path().to_str().expect("browser contract").into()),
        _ => unreachable!(),
    };
    let mut w = Worker::new(d.path(), "normal", selected.as_deref(), None);
    w.hello();
    let root = d.path().join("private-profile-payload");
    let r=w.call(json!({"protocol":1,"id":2,"operation":"open","backend":"cloak","options":{"user_data_dir":root,"headless":true},"init_scripts":[]}),0);
    assert_eq!(r["ok"], false);
    w.finish();
    assert!(!root.exists());
    assert!(
        !d.path().join("driver/driver-started").exists(),
        "vendor driver started before binary validation"
    );
}
fn timing(failure: bool, idle: bool, fragments: usize, legacy: Option<&str>) {
    let d = fresh();
    let mut w = Worker::new(
        d.path(),
        if failure { "launch-fail" } else { "normal" },
        None,
        legacy,
    );
    w.hello();
    let options = json!({"user_data_dir":d.path().join("private-profile-payload"),"headless":true,"extra_http_headers":{"x-auth":"private-password-payload"}});
    let open=w.call(json!({"protocol":1,"id":2,"operation":"open","backend":"playwright","options":options,"init_scripts":[]}),fragments);
    assert_eq!(open["ok"], !failure);
    if !failure {
        if idle {
            std::thread::park_timeout(Duration::from_millis(30));
            assert!(
                w.tree
                    .child_mut()
                    .try_wait()
                    .expect("browser contract")
                    .is_none()
            );
        }
        assert_eq!(
            w.call(json!({"protocol":1,"id":3,"operation":"close"}), 0)["ok"],
            true
        );
    }
    let exit = w.finish();
    assert_eq!(exit, i32::from(failure));
    let log = support::logs(d.path());
    for secret in ["private-profile-payload", "private-password-payload"] {
        assert!(!log.contains(secret), "payload in logs");
    }
    for label in if failure {
        vec!["hello", "open", "cleanup"]
    } else {
        vec!["hello", "open", "close", "cleanup"]
    } {
        let prefix = format!("SDK timing operation={label} elapsed_ns=");
        let text = log
            .split(&prefix)
            .nth(1)
            .unwrap_or_else(|| panic!("missing {label} timing {legacy:?}"));
        let number = text
            .split_whitespace()
            .next()
            .expect("browser contract")
            .trim_end_matches(',');
        assert!(number.parse::<u128>().is_ok());
    }
    let labels = log
        .lines()
        .filter_map(|line| line.split("SDK timing operation=").nth(1))
        .map(|line| line.split_whitespace().next().expect("timing label"))
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        if failure {
            vec!["hello", "open", "cleanup"]
        } else {
            vec!["hello", "open", "close", "cleanup"]
        }
    );
    support::wait(&d.path().join("driver/driver-stopped"));
    let rows = trace(&d.path().join("sdk-trace.jsonl"));
    let verdict = scheduling(&rows);
    if let Some(legacy) = legacy {
        let expected = match legacy {
            "poll" => "input readiness",
            "pump" => "fixed command delay",
            "fragment-wait" => "fixed fragment waits",
            _ => unreachable!(),
        };
        assert_eq!(
            verdict,
            Err(expected),
            "mutation must fail its scheduling contract"
        );
    } else {
        assert_eq!(verdict, Ok(()));
    }
    assert!(
        !std::fs::read_to_string(d.path().join("sdk-trace.jsonl"))
            .expect("bounded trace")
            .contains("private-")
    );
}
fn pipe(mode: &str) {
    if mode == "error" {
        bad_handle();
        return;
    }
    let mut c = std::process::Command::new(support::fixture());
    c.arg("pipe")
        .arg(if mode == "idle" {
            "idle"
        } else if mode == "arrival" {
            "arrival"
        } else if mode == "eof" {
            "eof"
        } else {
            "ready"
        })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut tree =
        numera_core::ownership::tree::OwnedTree::spawn(&mut c).expect("browser contract");
    let mut writer = tree.child_mut().stdin.take().expect("browser contract");
    let output = tree.child_mut().stdout.take().expect("browser contract");
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancel);
    let start = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut input = numera_core::native_input::Input::new(output, signal);
        let mut bytes = [0; 64];
        let result = input.read(&mut bytes);
        tx.send((result, bytes)).expect("browser contract");
    });
    if mode == "arrival" {
        writer.write_all(b"x").expect("browser contract");
        writer.flush().expect("browser contract");
    }
    if mode == "idle" || mode == "error" {
        cancel.store(true, Ordering::Release);
    }
    let (result, bytes) = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("native input cancellation/readiness bounded");
    reader.join().expect("browser contract");
    match mode {
        "idle" | "error" => {
            assert_eq!(
                result.expect_err("expected browser rejection").kind(),
                io::ErrorKind::ConnectionAborted
            )
        }
        "eof" => assert_eq!(result.expect("browser contract"), 0),
        _ => {
            let n = result.expect("browser contract");
            assert!(n > 0);
            assert!(
                std::str::from_utf8(&bytes[..n])
                    .expect("browser contract")
                    .contains("value")
            );
        }
    }
    assert!(start.elapsed() < Duration::from_secs(2));
    drop(writer);
    tree.settle(Duration::from_secs(2))
        .expect("browser contract");
}
fn framing(mode: &str) {
    match mode {
        "ready" => {
            assert_eq!(
                browser::adapter::frame(&mut Cursor::new(b"{\"value\":1}\n"))
                    .expect("browser contract"),
                Some(json!({"value":1}))
            );
        }
        "idle" => pipe("idle"),
        "buffered" => {
            let mut r = Cursor::new(b"{\"value\":1}\n{\"value\":2}\n");
            assert_eq!(
                browser::adapter::frame(&mut r).expect("browser contract"),
                Some(json!({"value":1}))
            );
            assert_eq!(
                browser::adapter::frame(&mut r).expect("browser contract"),
                Some(json!({"value":2}))
            );
        }
        "partial" => {
            let mut r = BufReader::with_capacity(2, Cursor::new(b"{\"value\":1}\n"));
            assert_eq!(
                browser::adapter::frame(&mut r).expect("browser contract"),
                Some(json!({"value":1}))
            );
        }
        "eof" => assert!(
            browser::adapter::frame(&mut Cursor::new(b""))
                .expect("browser contract")
                .is_none()
        ),
        "partial-eof" => assert_eq!(
            browser::adapter::frame(&mut Cursor::new(b"{\"value\":"))
                .expect_err("expected browser rejection")
                .kind(),
            io::ErrorKind::UnexpectedEof
        ),
        "oversized" => {
            assert!(browser::adapter::frame(&mut Cursor::new(vec![b'x'; 1_048_577])).is_err())
        }
        "invalid-json" => assert!(browser::adapter::frame(&mut Cursor::new(b"\xff\n")).is_err()),
        "nonobject" => assert!(browser::adapter::frame(&mut Cursor::new(b"[]\n")).is_err()),
        "read-error" | "would-block" => {
            struct Bad(io::ErrorKind);
            impl io::Read for Bad {
                fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                    Err(io::Error::from(self.0))
                }
            }
            let kind = if mode == "read-error" {
                io::ErrorKind::PermissionDenied
            } else {
                io::ErrorKind::WouldBlock
            };
            assert_eq!(
                browser::adapter::frame(&mut BufReader::new(Bad(kind)))
                    .expect_err("expected browser rejection")
                    .kind(),
                kind
            );
        }
        _ => unreachable!(),
    }
}
fn invalid_wait(value: f64) {
    struct NeverRead;
    let called = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::clone(&called);
    let result = Duration::try_from_secs_f64(value)
        .map_err(io::Error::other)
        .and_then(|wait| numera_core::native_input::Input::with_wait(NeverRead, cancelled, wait));
    assert!(result.is_err());
    assert!(!called.load(Ordering::Acquire));
}
source_test!(
    test_missing_selected_binary_refuses_before_vendor_import_absent,
    "tests/test_adapter_acquisition.py::test_missing_selected_binary_refuses_before_vendor_import[absent]",
    {
        missing("absent");
    }
);
source_test!(
    test_missing_selected_binary_refuses_before_vendor_import_empty,
    "tests/test_adapter_acquisition.py::test_missing_selected_binary_refuses_before_vendor_import[empty]",
    {
        missing("empty");
    }
);
source_test!(
    test_missing_selected_binary_refuses_before_vendor_import_missing,
    "tests/test_adapter_acquisition.py::test_missing_selected_binary_refuses_before_vendor_import[missing]",
    {
        missing("missing");
    }
);
source_test!(
    test_missing_selected_binary_refuses_before_vendor_import_directory,
    "tests/test_adapter_acquisition.py::test_missing_selected_binary_refuses_before_vendor_import[directory]",
    {
        missing("directory");
    }
);
source_test!(
    test_existing_native_selection_disables_updates_before_import_and_preserves_sdk_options,
    "tests/test_adapter_acquisition.py::test_existing_native_selection_disables_updates_before_import_and_preserves_sdk_options",
    {
        let d = fresh();
        let binary = d.path().join("selected-browser");
        fs::write(&binary, b"selected executable").expect("browser contract");
        let before = fs::read(&binary).expect("browser contract");
        let mut w = Worker::new(
            d.path(),
            "normal",
            Some(binary.to_str().expect("browser contract")),
            None,
        );
        w.hello();
        let options = json!({"user_data_dir":d.path().join("profile"),"headless":true,"browser_version":"146.0.7680.177.5"});
        assert_eq!(w.call(json!({"id":2,"protocol":1,"operation":"open","backend":"cloak","options":options,"init_scripts":[]}),0)["ok"],true);
        assert_eq!(
            w.call(json!({"id":3,"protocol":1,"operation":"close"}), 0)["ok"],
            true
        );
        w.finish();
        let policy: Value = serde_json::from_slice(
            &fs::read(d.path().join("driver/driver-policy.json"))
                .expect("vendor policy observation"),
        )
        .expect("vendor policy JSON");
        assert_eq!(
            policy["auto_update"], "false",
            "updates disabled before vendor process entry"
        );
        let req = support::requests(&d.path().join("driver"));
        let launch = req
            .iter()
            .find(|r| r["method"] == "launchPersistentContext")
            .expect("browser contract");
        assert_eq!(launch["params"]["executablePath"], json!(binary));
        assert_eq!(launch["params"]["headless"], true);
        assert_eq!(fs::read(binary).expect("browser contract"), before);
        assert_eq!(options["browser_version"], "146.0.7680.177.5");
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_false_0_none,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-False-0-None]",
    {
        timing(false, false, 0, None);
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_true_0_none,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-True-0-None]",
    {
        timing(false, true, 0, None);
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_true_false_0_none,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[True-False-0-None]",
    {
        timing(true, false, 0, None);
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_false_0_poll,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-False-0-poll]",
    {
        timing(false, false, 0, Some("poll"));
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_true_0_pump,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-True-0-pump]",
    {
        timing(false, true, 0, Some("pump"));
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_false_7_none,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-False-7-None]",
    {
        timing(false, false, 7, None);
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_true_false_7_none,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[True-False-7-None]",
    {
        timing(true, false, 7, None);
    }
);
source_test!(
    test_sdk_operation_timing_never_logs_payloads_false_false_7_fragment_wait,
    "tests/test_adapter_acquisition.py::test_sdk_operation_timing_never_logs_payloads[False-False-7-fragment-wait]",
    {
        timing(false, false, 7, Some("fragment-wait"));
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_ready,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[ready]",
    {
        framing("ready");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_idle,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[idle]",
    {
        framing("idle");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_buffered,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[buffered]",
    {
        framing("buffered");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_partial,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[partial]",
    {
        framing("partial");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_eof,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[eof]",
    {
        framing("eof");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_partial_eof,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[partial-eof]",
    {
        framing("partial-eof");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_oversized,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[oversized]",
    {
        framing("oversized");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_invalid_json,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[invalid-json]",
    {
        framing("invalid-json");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_nonobject,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[nonobject]",
    {
        framing("nonobject");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_read_error,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[read-error]",
    {
        framing("read-error");
    }
);
source_test!(
    test_frame_input_preserves_readiness_framing_and_eof_would_block,
    "tests/test_adapter_acquisition.py::test_frame_input_preserves_readiness_framing_and_eof[would-block]",
    {
        framing("would-block");
    }
);
source_test!(
    test_windows_frame_wait_stops_on_readiness_or_its_deadline_ready,
    "tests/test_adapter_acquisition.py::test_windows_frame_wait_stops_on_readiness_or_its_deadline[ready]",
    {
        pipe("ready");
    }
);
source_test!(
    test_windows_frame_wait_stops_on_readiness_or_its_deadline_arrival,
    "tests/test_adapter_acquisition.py::test_windows_frame_wait_stops_on_readiness_or_its_deadline[arrival]",
    {
        pipe("arrival");
    }
);
source_test!(
    test_windows_frame_wait_stops_on_readiness_or_its_deadline_idle,
    "tests/test_adapter_acquisition.py::test_windows_frame_wait_stops_on_readiness_or_its_deadline[idle]",
    {
        pipe("idle");
    }
);
source_test!(
    test_windows_frame_wait_stops_on_readiness_or_its_deadline_eof,
    "tests/test_adapter_acquisition.py::test_windows_frame_wait_stops_on_readiness_or_its_deadline[eof]",
    {
        pipe("eof");
    }
);
source_test!(
    test_windows_frame_wait_stops_on_readiness_or_its_deadline_error,
    "tests/test_adapter_acquisition.py::test_windows_frame_wait_stops_on_readiness_or_its_deadline[error]",
    {
        pipe("error");
    }
);
source_test!(
    test_frame_input_rejects_invalid_wait_before_pipe_access_1,
    "tests/test_adapter_acquisition.py::test_frame_input_rejects_invalid_wait_before_pipe_access[-1]",
    {
        invalid_wait(-1.0);
    }
);
source_test!(
    test_frame_input_rejects_invalid_wait_before_pipe_access_0_026,
    "tests/test_adapter_acquisition.py::test_frame_input_rejects_invalid_wait_before_pipe_access[0.026]",
    {
        invalid_wait(0.026);
    }
);
source_test!(
    test_frame_input_rejects_invalid_wait_before_pipe_access_inf,
    "tests/test_adapter_acquisition.py::test_frame_input_rejects_invalid_wait_before_pipe_access[inf]",
    {
        invalid_wait(f64::INFINITY);
    }
);
source_test!(
    test_frame_input_rejects_invalid_wait_before_pipe_access_nan,
    "tests/test_adapter_acquisition.py::test_frame_input_rejects_invalid_wait_before_pipe_access[nan]",
    {
        invalid_wait(f64::NAN);
    }
);

fn bad_handle() {
    struct Bad;
    impl Read for Bad {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("invalid handle must not use buffered read")
        }
    }
    #[cfg(unix)]
    impl std::os::fd::AsRawFd for Bad {
        fn as_raw_fd(&self) -> std::os::fd::RawFd {
            i32::MAX
        }
    }
    #[cfg(windows)]
    impl std::os::windows::io::AsRawHandle for Bad {
        fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
            usize::MAX as *mut std::ffi::c_void
        }
    }
    let mut input = numera_core::native_input::Input::new(Bad, Arc::new(AtomicBool::new(false)));
    assert!(input.read(&mut [0; 1]).is_err());
}

fn trace(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn scheduling(rows: &[Value]) -> Result<(), &'static str> {
    assert!(rows.len() <= 128, "test trace must remain bounded");
    if rows
        .iter()
        .any(|row| row["event"] == "fragment_delay" && row["ms"] != 0)
    {
        return Err("fixed fragment waits");
    }
    if rows
        .iter()
        .any(|row| row["event"] == "idle_delay" && row["ms"] != 0)
    {
        return Err("fixed command delay");
    }
    if rows
        .iter()
        .any(|row| row["event"] == "input_wait" && row["ms"] != 25)
    {
        return Err("input readiness");
    }
    assert!(
        rows.iter().any(|row| row["event"] == "input_wait"),
        "actual native waits were not observed"
    );
    Ok(())
}
