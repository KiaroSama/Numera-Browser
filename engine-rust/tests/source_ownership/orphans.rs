use super::*;
#[path = "windows_cleanup.rs"]
mod windows_cleanup;

pub(super) struct RawChild(pub(super) std::process::Child);
impl Drop for RawChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Cooperative {
    release: PathBuf,
    pid: u32,
    token: Option<u64>,
}
impl Drop for Cooperative {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"go");
        let end = Instant::now() + Duration::from_secs(2);
        while process_running(self.pid, self.token).unwrap_or(true) && Instant::now() < end {
            thread::sleep(Duration::from_millis(5));
        }
    }
}
struct Launcher {
    child: RawChild,
    record: Value,
    browser: u32,
    token: Option<u64>,
}
impl Launcher {
    fn settle(&mut self) -> io::Result<()> {
        if recorded_tree::settle(&self.record, Duration::from_secs(2))? {
            Ok(())
        } else {
            Err(io::Error::other("fixture tree did not settle"))
        }
    }
}
impl Drop for Launcher {
    fn drop(&mut self) {
        let _ = self.child.0.kill();
        let _ = self.child.0.wait();
        let _ = self.settle();
    }
}
fn killed_launcher(root: &Path) -> Result<Launcher, Box<dyn std::error::Error>> {
    let mut command = command("session")?;
    command
        .arg(root)
        .arg("p")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = RawChild(command.spawn()?);
    wait_file(&root.join("session-ready"))?;
    let mut record = match claims::read(root, "p") {
        claims::Claim::Record(record) => record,
        _ => return Err("launcher never published its tree".into()),
    };
    record["pidToken"] = record["token"].take();
    let browser: u32 = fs::read_to_string(root.join("descendant"))?.parse()?;
    let token = identity::started_at(browser)?;
    let mut owner = Launcher {
        child,
        record,
        browser,
        token,
    };
    assert!(process_running(browser, token)?);
    owner.child.0.kill()?;
    owner.child.0.wait()?;
    Ok(owner)
}
#[cfg(unix)]
#[allow(unsafe_code)]
fn host_group() -> i32 {
    // SAFETY: read-only scalar kernel identity with no pointer arguments.
    unsafe { libc::getpgrp() }
}

source_test!(
    session_ownership_a_killed_launcher_does_not_release_the_profile,
    "tests/test_session_ownership.py::test_a_killed_launcher_does_not_release_the_profile",
    {
        let t = temp("launcher-death-claim")?;
        profile(t.path(), "p")?;
        let mut owner = killed_launcher(t.path())?;
        let claim = claims::read(t.path(), "p");
        if cfg!(windows) {
            let end = Instant::now() + Duration::from_secs(2);
            while claims::state(&claims::read(t.path(), "p"), "source-host") != claims::State::Free
            {
                assert!(Instant::now() < end, "closed Windows Job remained occupied");
                thread::sleep(Duration::from_millis(5));
            }
            let _session = Session::admit(t.path(), "p", "source-host")?;
        } else {
            assert_eq!(claims::state(&claim, "source-host"), claims::State::Busy);
            assert!(Session::admit(t.path(), "p", "source-host").is_err());
            assert!(process_running(owner.browser, owner.token)?);
            owner.settle()?;
            assert_eq!(
                claims::state(&claims::read(t.path(), "p"), "source-host"),
                claims::State::Free
            );
            let _session = Session::admit(t.path(), "p", "source-host")?;
        }
        Ok(())
    }
);
source_test!(
    session_ownership_the_control,
    "tests/test_session_ownership.py::test_the_control",
    {
        let t = temp("launcher-death-lease-control")?;
        profile(t.path(), "p")?;
        let _owner = killed_launcher(t.path())?;
        let path = SessionLease::path_for(t.path(), "p")?;
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            match SessionLease::acquire(&path) {
                Ok(lease) => {
                    drop(lease);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < end, "OS lease outlived killed launcher");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    session_ownership_stop_reports_a_session_whose_launcher_is_gone,
    "tests/test_session_ownership.py::test_stop_reports_a_session_whose_launcher_is_gone",
    {
        let t = temp("orphaned-cli-stop")?;
        profile(t.path(), "p")?;
        let owner = killed_launcher(t.path())?;
        assert!(process_running(owner.browser, owner.token)?);
        let outcome = super::status::cli_stop(t.path(), "p")?;
        assert_eq!(outcome.exit, 1);
        assert_eq!(outcome.payload["ok"], false);
        assert!(
            outcome.payload["error"]
                .as_str()
                .is_some_and(|error| error.contains("another process"))
        );
        assert!(
            process_running(owner.browser, owner.token)?,
            "foreign-owner refusal killed the orphan itself"
        );
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    session_ownership_a_launcher_that_never_led_a_group_still_owns_its_browser,
    "tests/test_session_ownership.py::test_a_launcher_that_never_led_a_group_still_owns_its_browser",
    {
        let t = temp("nonleader-launcher-session")?;
        profile(t.path(), "p")?;
        let owner = killed_launcher(t.path())?;
        assert_eq!(owner.record["tree"]["kind"], "pgid");
        assert_ne!(
            owner.record["tree"]["group"].as_i64(),
            Some(i64::from(host_group()))
        );
        assert!(process_running(owner.browser, owner.token)?);
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_legacy_leader_only_claim_survives_the_death_of_its_leader,
    "tests/test_session_ownership.py::test_a_legacy_leader_only_claim_survives_the_death_of_its_leader",
    {
        let t = temp("dead-legacy-leader")?;
        profile(t.path(), "p")?;
        let mut pid = None;
        let mut child = command("emit")?;
        child.args(["done", "0"]);
        let output = capture::run_observed(
            &mut child,
            limit(Duration::from_secs(3)),
            || false,
            |event, _| {
                if let capture::Observation::Spawned { record } = event {
                    pid = record["pid"].as_u64().map(|pid| pid as u32);
                }
            },
        )?;
        assert_eq!(output.exit, 0);
        let pid = pid.ok_or("dead leader was not spawned")?;
        assert!(!identity::alive(pid, None)?);
        write_json(
            &t.path().join("sessions/p.json"),
            &json!({"session":"legacy","pid":pid,"token":12345,"tree":{"kind":"","group":null},"what":"browser"}),
        )?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Unknown
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(!claims::clear(t.path(), "p", "source-host")?);
        Ok(())
    }
);
source_test!(
    session_ownership_an_unrelated_host_child_belongs_to_neither_session,
    "tests/test_session_ownership.py::test_an_unrelated_host_child_belongs_to_neither_session",
    {
        let t = temp("unrelated-host-child")?;
        profile(t.path(), "p")?;
        let mut command = command("sleep")?;
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut stranger = RawChild(command.spawn()?);
        let stranger_token = identity::started_at(stranger.0.id())?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut browser = super::command("sleep")?;
        browser.stdout(Stdio::null()).stderr(Stdio::null());
        session.spawn(&mut browser, "browser")?;
        assert_ne!(
            session.tree_record().ok_or("tree absent")?["pid"],
            stranger.0.id()
        );
        session.finish(Duration::from_secs(1))?;
        assert!(stranger.0.try_wait()?.is_none());
        assert!(identity::alive(stranger.0.id(), stranger_token)?);
        drop(session);
        let _next = Session::admit(t.path(), "p", "source-host")?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_session_tree_that_outlives_a_clean_exit_keeps_the_profile,
    "tests/test_session_ownership.py::test_a_session_tree_that_outlives_a_clean_exit_keeps_the_profile",
    {
        let t = temp("clean-leader-live-tree")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("tree")?;
        child
            .arg(t.path())
            .arg("exit")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        session.spawn(&mut child, "browser")?;
        wait_file(&t.path().join("leader"))?;
        let status = session.child_mut()?.wait()?;
        assert!(status.success());
        let browser: u32 = fs::read_to_string(t.path().join("descendant"))?.parse()?;
        let token = identity::started_at(browser)?;
        assert!(process_running(browser, token)?);
        assert!(
            session.settled_exit()?.is_none(),
            "clean leader exit was mistaken for full-tree completion"
        );
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        #[cfg(windows)]
        let leaf = windows_cleanup::LeafWitness::retain(
            browser,
            token,
            &session.tree_record().ok_or("Session Job missing")?,
        )?;
        session.finish(Duration::from_secs(1))?;
        #[cfg(windows)]
        leaf.assert_exited();
        assert!(!process_running(browser, token)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        windows_cleanup::policy_controls()?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_browser_that_would_not_close_keeps_the_profile,
    "tests/test_session_ownership.py::test_a_browser_that_would_not_close_keeps_the_profile",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refuse;
        impl Backend for Refuse {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled session cleanup refusal",
                )))
            }
        }
        let t = temp("session-retains-live-browser")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let pid = session.spawn(&mut child, "browser")?.pid();
        let token = identity::started_at(pid)?;
        assert!(
            with_backend(Arc::new(Refuse), || session
                .finish(Duration::from_millis(80)))
            .is_err()
        );
        assert!(process_running(pid, token)?);
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        session.finish(Duration::from_secs(1))?;
        assert!(!process_running(pid, token)?);
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    session_ownership_a_tree_that_cannot_be_taken_leaves_the_host_where_it_was,
    "tests/test_session_ownership.py::test_a_tree_that_cannot_be_taken_leaves_the_host_where_it_was",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refuse;
        impl Backend for Refuse {
            fn before_spawn(&self) -> Option<io::Result<()>> {
                Some(Err(io::Error::other("controlled tree admission refusal")))
            }
        }
        let before = host_group();
        let token = identity::started_at(std::process::id())?;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        assert!(with_backend(Arc::new(Refuse), || OwnedTree::spawn(&mut child)).is_err());
        assert_eq!(host_group(), before);
        assert_eq!(identity::started_at(std::process::id())?, token);
        Ok(())
    }
);
source_test!(
    session_ownership_the_rendezvous_helper_is_untouched,
    "tests/test_session_ownership.py::test_the_rendezvous_helper_is_untouched",
    {
        let t = temp("native-rendezvous-control")?;
        let ready = t.path().join("barrier-ready");
        let leave = t.path().join("barrier-leave");
        let mut child = command("hold-lease")?;
        child
            .arg(t.path())
            .arg("p")
            .arg(&ready)
            .arg(&leave)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let path = SessionLease::path_for(t.path(), "p")?;
        assert_eq!(
            SessionLease::acquire(&path)
                .err()
                .ok_or("fixture did not hold lease")?
                .kind(),
            io::ErrorKind::WouldBlock
        );
        fs::write(&leave, b"released")?;
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = owner.child_mut().try_wait()? {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < end, "native rendezvous never released");
            thread::sleep(Duration::from_millis(5));
        }
        owner.settle(Duration::from_secs(1))?;
        let lease = SessionLease::acquire(&path)?;
        drop(lease);
        drop(owner);
        t.cleanup()?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_spawned_session_tells_its_child_which_tree_it_is_in,
    "tests/test_session_ownership.py::test_a_spawned_session_tells_its_child_which_tree_it_is_in",
    {
        let t = temp("native-child-tree-awareness")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("membership")?;
        child
            .arg(t.path())
            .arg("p")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        session.spawn(&mut child, "membership")?;
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(code) = session.settled_exit()? {
                assert_eq!(code, 0);
                break;
            }
            assert!(
                Instant::now() < end,
                "child did not observe its published owner"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let mut bytes = Vec::new();
        use std::io::Read;
        session
            .child_mut()?
            .stdout
            .take()
            .ok_or("membership output absent")?
            .take(4096)
            .read_to_end(&mut bytes)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(value["claim_preceded_observation"], true);
        assert_eq!(value["same_kernel_tree"], true);
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    owned_tree_the_control,
    "tests/test_owned_tree.py::test_the_control",
    {
        let t = temp("unowned-orphan-control")?;
        let mut child = command("plain-orphan")?;
        child
            .arg(t.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut parent = RawChild(child.spawn()?);
        wait_file(&t.path().join("orphan-pid"))?;
        let pid: u32 = fs::read_to_string(t.path().join("orphan-pid"))?.parse()?;
        let token = identity::started_at(pid)?;
        let _release = Cooperative {
            release: t.path().join("orphan-release"),
            pid,
            token,
        };
        assert!(parent.0.wait()?.success());
        assert!(
            process_running(pid, token)?,
            "fixture failed to reproduce the unowned survivor"
        );
        let lock = DataLock::new(t.path().join("orphan.lock"))?;
        assert_eq!(
            lock.acquire(Duration::from_millis(60))
                .err()
                .ok_or("orphan released lock early")?
                .kind(),
            io::ErrorKind::TimedOut
        );
        fs::write(t.path().join("orphan-release"), b"go")?;
        let end = Instant::now() + Duration::from_secs(2);
        while process_running(pid, token)? {
            assert!(Instant::now() < end, "cooperative orphan did not exit");
            thread::sleep(Duration::from_millis(5));
        }
        let _guard = lock.acquire(Duration::from_secs(1))?;
        Ok(())
    }
);
