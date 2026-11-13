use super::*;
source_test!(
    data_lock_ownership_thread_and_os_wait_share_one_budget,
    "tests/test_data_lock_ownership.py::test_thread_and_os_wait_share_one_budget",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Hold {
            target: PathBuf,
            arrived: mpsc::Sender<()>,
            release: std::sync::Mutex<mpsc::Receiver<()>>,
        }
        impl Backend for Hold {
            fn before_os_lock(&self, path: &Path) -> Option<io::Result<()>> {
                if path == self.target {
                    self.arrived.send(()).expect("local owner reached OS stage");
                    self.release
                        .lock()
                        .expect("release")
                        .recv_timeout(Duration::from_secs(3))
                        .expect("local owner released");
                }
                None
            }
        }
        let t = temp("shared-thread-os-budget")?;
        let path = t.path().join("contended.lock");
        let ready = t.path().join("os-owner-ready");
        let leave = t.path().join("os-owner-leave");
        let mut child = command("hold-lock")?;
        child
            .arg(&path)
            .arg(&ready)
            .arg(&leave)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let lock = Arc::new(DataLock::new(path.clone())?);
        let (arrived, observed) = mpsc::channel();
        let (release, waiting) = mpsc::channel();
        let pause = Arc::new(Hold {
            target: path,
            arrived,
            release: std::sync::Mutex::new(waiting),
        });
        let first_lock = lock.clone();
        let first = thread::spawn(move || {
            with_backend(pause, || {
                first_lock
                    .acquire(Duration::ZERO)
                    .map(drop)
                    .map_err(|error| error.kind())
            })
        });
        observed.recv_timeout(Duration::from_secs(2))?;
        let (began, entered) = mpsc::channel();
        let next = lock.clone();
        let second = thread::spawn(move || {
            let begin = Instant::now();
            began.send(()).expect("contender entered");
            let result = next
                .acquire(Duration::from_millis(300))
                .map(drop)
                .map_err(|error| error.kind());
            (result, begin.elapsed())
        });
        entered.recv_timeout(Duration::from_secs(1))?;
        thread::sleep(Duration::from_millis(200));
        release.send(())?;
        assert_eq!(
            first.join().expect("local owner"),
            Err(io::ErrorKind::TimedOut)
        );
        let (outcome, elapsed) = second.join().expect("two-stage contender");
        assert_eq!(outcome, Err(io::ErrorKind::TimedOut));
        assert!(
            elapsed < Duration::from_millis(430),
            "OS stage restarted the full300ms after thread wait: {elapsed:?}"
        );
        fs::write(&leave, b"go")?;
        owner.child_mut().wait()?;
        owner.settle(Duration::from_secs(1))?;
        let _guard = lock.acquire(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    data_lock_ownership_wrong_thread_exit_cannot_change_depth_or_release_os_lock,
    "tests/test_data_lock_ownership.py::test_wrong_thread_exit_cannot_change_depth_or_release_os_lock",
    {
        let t = temp("non-send-lock-guard")?;
        let lock = DataLock::transaction(t.path())?;
        let guard = lock.acquire(Duration::from_secs(1))?;
        let source = t.path().join("wrong_thread.rs");
        fs::write(
            &source,
            r#"use numera_core::storage::locks::DataLock;fn misuse(lock:&DataLock){let guard=lock.acquire(std::time::Duration::ZERO).unwrap();std::thread::scope(|scope|{scope.spawn(move||drop(guard));});}"#,
        )?;
        let deps = std::env::current_exe()?
            .parent()
            .ok_or("test deps missing")?
            .to_owned();
        let mut compiler =
            Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        compiler
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&source)
            .arg("--extern")
            .arg(format!(
                "numera_core={}",
                library(&deps, "numera_core")?.display()
            ))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-o")
            .arg(t.path().join("misuse.rmeta"));
        let answer = capture::run(&mut compiler, limit(Duration::from_secs(30)))?;
        assert_ne!(
            answer.exit, 0,
            "native lock guard crossed a thread boundary"
        );
        let diagnostic = String::from_utf8_lossy(&answer.stderr);
        assert!(
            diagnostic.contains("cannot be sent between threads safely"),
            "compile failed for an unrelated reason: {diagnostic}"
        );
        assert!(diagnostic.contains("LockGuard") || diagnostic.contains("Rc<()>"));
        assert!(lock.owned_by_current_thread()?);
        let mut child = command("lock")?;
        child.arg(lock.path());
        let blocked = capture::run(&mut child, limit(Duration::from_secs(3)))?;
        assert_eq!(
            blocked.stdout, b"timeout\n",
            "foreign-thread compile attempt released the OS lock"
        );
        drop(guard);
        assert!(!lock.owned_by_current_thread()?);
        let entered = capture::run(&mut child, limit(Duration::from_secs(3)))?;
        assert_eq!(entered.stdout, b"entered\n");
        Ok(())
    }
);
