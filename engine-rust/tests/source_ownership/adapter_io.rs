use super::*;

fn late_io(index: usize, dispose: bool) -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Late {
        index: usize,
        arrived: mpsc::Sender<()>,
        release: std::sync::Mutex<mpsc::Receiver<()>>,
    }
    impl Backend for Late {
        fn adapter_worker_end(&self, index: usize) {
            if index == self.index {
                self.arrived.send(()).expect("I/O completion arrival");
                self.release
                    .lock()
                    .expect("I/O release")
                    .recv_timeout(Duration::from_secs(8))
                    .expect("I/O completion release");
            }
        }
    }
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(release) = self.0.take() {
                let _ = release.send(());
            }
        }
    }
    let t = temp("late-native-adapter-io")?;
    profile(t.path(), "p")?;
    write_json(
        &t.path().join("evaluation.json"),
        &json!({"stdout":"OUT","stderr":"ERR"}),
    )?;
    let (arrived, observed) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let mut release = Release(Some(release));
    let hook = Arc::new(Late {
        index,
        arrived,
        release: std::sync::Mutex::new(waiting),
    });
    let mut adapter = with_backend(hook, || {
        super::sessions::open_adapter(t.path(), "p", "playwright")
    })?;
    let expected = adapter.call("evaluate", Default::default(), Duration::from_secs(1))?;
    assert_eq!(expected["value"], json!({"stdout":"OUT","stderr":"ERR"}));
    let metadata = adapter.metadata().clone();
    let browser: u32 = fs::read_to_string(t.path().join("browser-pid"))?.parse()?;
    let token = identity::started_at(browser)?;
    let begin = Instant::now();
    let failure = adapter
        .finish(Duration::from_millis(120))
        .expect_err("late I/O worker was discarded");
    assert!(
        begin.elapsed() < Duration::from_millis(600),
        "I/O join ignored remaining cleanup budget"
    );
    assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
    assert!(failure.to_string().contains(if index == 0 {
        "input writer"
    } else {
        "output reader"
    }));
    observed.recv_timeout(Duration::from_secs(1))?;
    assert!(
        !process_running(browser, token)?,
        "native EOF control never ended its browser"
    );
    assert_eq!(
        adapter.metadata(),
        &metadata,
        "pending disposal changed captured metadata"
    );
    if dispose {
        let begin = Instant::now();
        drop(adapter);
        assert!(begin.elapsed() < Duration::from_millis(2600));
        assert!(
            !numera_core::browser::adapter::retry_retained(Duration::ZERO).is_empty(),
            "dropping a pending adapter detached its I/O worker"
        );
        release.0.take().ok_or("release missing")?.send(())?;
        assert!(
            numera_core::browser::adapter::retry_retained(Duration::from_secs(1)).is_empty(),
            "released I/O worker was not joined on retry"
        );
    } else {
        release.0.take().ok_or("release missing")?.send(())?;
        adapter.finish(Duration::from_secs(1))?;
        assert_eq!(adapter.metadata(), &metadata);
        adapter.finish(Duration::ZERO)?;
    }
    assert_eq!(expected["value"], json!({"stdout":"OUT","stderr":"ERR"}));
    Ok(())
}

source_test!(
    helper_retry_completion_real_eof_and_late_thread_exit_can_finish_on_retry_63d42d26,
    "tests/test_helper_retry_completion.py::test_real_eof_and_late_thread_exit_can_finish_on_retry[stdout]",
    { late_io(1, false) }
);
source_test!(
    helper_retry_completion_real_eof_and_late_thread_exit_can_finish_on_retry_7e6b710b,
    "tests/test_helper_retry_completion.py::test_real_eof_and_late_thread_exit_can_finish_on_retry[stderr]",
    { late_io(0, false) }
);
source_test!(
    helper_completion_regression_helper_stays_registered_until_stream_disposal,
    "tests/test_helper_completion_regression.py::test_helper_stays_registered_until_stream_disposal",
    { late_io(1, true) }
);
