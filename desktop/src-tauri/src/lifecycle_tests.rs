#[cfg(test)]
mod tests {
    use super::super::*;

    /// Answer one request with `response`, then close. Returns the port.
    fn stub(response: &'static str) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut scratch = [0u8; 1024];
                let _ = sock.read(&mut scratch);
                let _ = sock.write_all(response.as_bytes());
            }
        });
        port
    }

    #[test]
    fn accepts_the_engines_own_health_payload() {
        // Exactly what `numera serve` returns from `/api/ready`, verified
        // against a live server. This stub still carried the old
        // `/api/health` payload, which has no `service` field - so it
        // asserted a contract `healthy()` had stopped accepting, and
        // nothing ever ran it to notice.
        let port = stub("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true,\"service\":\"numera-browser\",\"version\":\"0.11.0\",\"authRequired\":false}");
        assert!(healthy(port));
    }

    #[test]
    fn rejects_a_200_from_something_that_is_not_numera() {
        // The whole reason the body is checked: another app can own the port.
        let port = stub("HTTP/1.1 200 OK\r\n\r\n<html>some other server</html>");
        assert!(!healthy(port));
    }

    #[test]
    fn rejects_a_non_200() {
        let port = stub("HTTP/1.1 503 Service Unavailable\r\n\r\n{\"ok\":true}");
        assert!(!healthy(port));
    }

    #[test]
    fn rejects_a_closed_port() {
        let port = free_port().unwrap(); // reserved then released; nothing listening
        assert!(!healthy(port));
    }

    /// The whole point of the job object: the child must die when the last
    /// handle to the job closes, which is what happens if the shell is killed.
    #[cfg(windows)]
    #[test]
    fn a_job_kills_its_child_when_the_last_handle_closes() {
        let guard = job::Job::new().expect("could not create a job object");
        let mut child = Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the stand-in child");
        assert!(
            guard.adopt(&child),
            "the child was not adopted into the job"
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "the child died on its own"
        );

        drop(guard);

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        panic!("the child outlived the job");
    }

    // --- where the portable build keeps its data ----------------------------

    /// A scratch directory of our own: empty at the start, gone at the end,
    /// however the test ended. `Drop` also puts back anything a case did to
    /// its permissions, so a panicked assertion cannot leave an undeletable
    /// directory behind in the system temp folder.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("numera-n15-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("could not create the scratch directory");
            Scratch(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            #[cfg(windows)]
            {
                let _ = Command::new("attrib")
                    .arg("-R")
                    .arg(self.0.join("*"))
                    .args(["/S", "/D"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if let Some(account) = account() {
                    let _ = icacls(&self.0, &["/remove:d", &account, "/T", "/C", "/Q"]);
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for dir in [self.0.clone(), self.0.join("data")] {
                    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(windows)]
    const HOME_VAR: &str = "USERPROFILE";
    #[cfg(not(windows))]
    const HOME_VAR: &str = "HOME";

    /// The environment these cases need: `NUMERA_DATA_DIR` cleared, and a home
    /// of our own so the per-user fallback can be asserted without ever
    /// touching the real one. Under a lock, because environment variables are
    /// process-wide and `cargo test` runs these on parallel threads.
    struct Env {
        home: Scratch,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new(name: &str) -> Self {
            // A failing case poisons the lock; the next one still needs it.
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let home = Scratch::new(&format!("{name}-home"));
            // SAFETY: that lock is what makes this the only thread touching
            // the environment. `home` is declared first so it is removed
            // before the lock is released.
            unsafe {
                std::env::remove_var("NUMERA_DATA_DIR");
                std::env::set_var(HOME_VAR, home.path());
            }
            Env { home, _lock: lock }
        }

        fn home(&self) -> &std::path::Path {
            self.home.path()
        }
    }

    /// The account this test runs as, in a form `icacls` accepts.
    #[cfg(windows)]
    fn account() -> Option<String> {
        let user = std::env::var("USERNAME").ok().filter(|u| !u.is_empty())?;
        match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => Some(format!("{domain}\\{user}")),
            _ => Some(user),
        }
    }

    #[cfg(windows)]
    fn icacls(dir: &std::path::Path, args: &[&str]) -> bool {
        Command::new("icacls")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// The shell used to pass `--data-dir` unconditionally, and an explicit
    /// argument outranks `NUMERA_DATA_DIR` in the engine's resolution order -
    /// so a user who set it from the desktop app was quietly ignored. It is
    /// answered first, before the portable location is so much as probed.
    #[test]
    fn an_environment_override_is_left_to_the_engine() {
        let env = Env::new("env-override");
        let root = Scratch::new("env-override-root");
        // SAFETY: `Env` holds the lock that serialises these cases.
        unsafe { std::env::set_var("NUMERA_DATA_DIR", env.home().join("elsewhere")) };

        // A perfectly writable portable root: the override still wins.
        assert!(
            portable_data_dir(root.path()).is_none(),
            "the shell overruled the user's own NUMERA_DATA_DIR"
        );
        assert!(
            !root.path().join("data").exists(),
            "the portable location was probed despite an explicit override"
        );
    }

    /// The ordinary portable case: data beside the shell, never inside
    /// engine/, which an update replaces wholesale.
    #[test]
    fn a_writable_portable_root_keeps_data_beside_the_shell() {
        let _env = Env::new("writable");
        let root = Scratch::new("writable-root");

        let chosen = portable_data_dir(root.path()).expect("a writable root got no data dir");

        assert_eq!(chosen, root.path().join("data"));
        assert!(chosen.is_dir(), "the chosen directory was not created");
        assert!(
            !chosen.starts_with(root.path().join("engine")),
            "data was placed inside the replaceable engine directory"
        );
        // The probe clears up after itself. A fresh data directory that starts
        // life with a stray dotfile in it ends up in the user's next backup.
        assert!(
            std::fs::read_dir(&chosen).unwrap().next().is_none(),
            "the write probe left a file behind"
        );
        // And the answer is load-bearing rather than inferred: it takes a file.
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the chosen directory refused a real write");
    }

    #[test]
    fn an_absent_portable_root_is_created_and_chosen() {
        let _env = Env::new("absent");
        let root = Scratch::new("absent-root");
        let install = root.path().join("not").join("there").join("yet");

        let chosen = portable_data_dir(&install).expect("a creatable root got no data dir");

        assert_eq!(chosen, install.join("data"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the chosen directory refused a real write");
    }

    /// A portable install normally lives somewhere with spaces in the path,
    /// like `C:\Program Files\Numera Browser`. Nothing here quotes or splits a
    /// string, and this is what keeps it that way.
    #[test]
    fn a_path_with_spaces_is_chosen_and_written_like_any_other() {
        let _env = Env::new("spaced");
        let root = Scratch::new("with spaces");
        let install = root.path().join("Program Files").join("Numera Browser");

        let chosen = portable_data_dir(&install).expect("a path with spaces got no data dir");

        assert_eq!(chosen, install.join("data"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("a directory whose path contains spaces refused a real write");
    }

    /// Windows: the read-only *attribute* on a directory is not an answer
    /// about writability. NTFS creates files inside one perfectly happily, so
    /// rejecting such a directory would move a working portable install to the
    /// user's home for no reason. The decision has to agree with what the
    /// filesystem actually does, which is what this asks it directly.
    #[cfg(windows)]
    #[test]
    fn a_read_only_attribute_is_not_an_answer_about_writability() {
        let _env = Env::new("read-only-attr");
        let root = Scratch::new("read-only-attr-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let marked = Command::new("attrib")
            .arg("+R")
            .arg(&data)
            .arg("/D")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !marked || !std::fs::metadata(&data).unwrap().permissions().readonly() {
            eprintln!(
                "SKIPPED a_read_only_attribute_is_not_an_answer_about_writability: the \
                 read-only attribute would not stay on {}",
                data.display()
            );
            return;
        }

        // What the filesystem says, asked independently of the code under test.
        let witness = data.join("independent-witness");
        let creatable = std::fs::File::create(&witness).is_ok();
        let _ = std::fs::remove_file(&witness);

        let chosen = portable_data_dir(root.path());
        if creatable {
            assert_eq!(
                chosen.as_deref(),
                Some(data.as_path()),
                "a directory that accepts files was rejected over an attribute flag"
            );
        } else {
            assert_ne!(
                chosen.as_deref(),
                Some(data.as_path()),
                "a directory that refuses files was chosen anyway"
            );
        }
    }

    /// The same case where the permission bits really do decide it.
    #[cfg(unix)]
    #[test]
    fn a_read_only_portable_root_falls_back_to_the_per_user_location() {
        use std::os::unix::fs::PermissionsExt;

        let env = Env::new("read-only");
        let root = Scratch::new("read-only-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555)).unwrap();

        let witness = data.join("independent-witness");
        if std::fs::File::create(&witness).is_ok() {
            let _ = std::fs::remove_file(&witness);
            eprintln!(
                "SKIPPED a_read_only_portable_root_falls_back_to_the_per_user_location: mode \
                 0o555 did not deny file creation - running as root?"
            );
            return;
        }

        let chosen = portable_data_dir(root.path()).expect("an unwritable root got no data dir");

        assert_eq!(chosen, env.home().join(".numera"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// Windows: the case the read-only attribute cannot express, and the
    /// finding itself. A DENY ACE refuses file creation while `create_dir_all`
    /// still answers `Ok(())`, because the directory is already there - so the
    /// shell handed the engine a `--data-dir` it cannot write, and an explicit
    /// one takes away the fallback the engine would otherwise have found.
    #[cfg(windows)]
    #[test]
    fn an_acl_that_denies_file_creation_falls_back_to_the_per_user_location() {
        let env = Env::new("acl-denied");
        let root = Scratch::new("acl-denied-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let Some(account) = account() else {
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 no USERNAME in the environment to write a DENY ACE for"
            );
            return;
        };
        // WD is "create a file in this directory", AD "create a subdirectory".
        if !icacls(&data, &["/deny", &format!("{account}:(WD,AD)")]) {
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 icacls could not write a DENY ACE on {}",
                data.display()
            );
            return;
        }
        // Prove the condition exists before trusting what the code says of it.
        let witness = data.join("independent-witness");
        if std::fs::File::create(&witness).is_ok() {
            let _ = std::fs::remove_file(&witness);
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 the DENY ACE did not deny - a privileged or specially granted account"
            );
            return;
        }

        assert!(
            std::fs::create_dir_all(&data).is_ok(),
            "the trap this exists for is gone: create_dir_all now reports the denial"
        );

        let chosen = portable_data_dir(root.path()).expect("an unwritable root got no data dir");

        assert_eq!(chosen, env.home().join(".numera"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// An install the shell cannot write - Program Files, a read-only mount.
    ///
    /// This asserted `None` and called it deferring to the engine, which reads
    /// well and is wrong: with no `--data-dir`, a frozen engine's own portable
    /// default is `data/` beside its *binary*, and the binary sits in
    /// `engine/` - the directory an update replaces wholesale. Silence put the
    /// user's profiles exactly where the next update would delete them.
    #[test]
    fn an_unwritable_root_names_the_per_user_location_instead() {
        let env = Env::new("unwritable");
        let root = Scratch::new("unwritable-root");
        // A path under a FILE cannot be created as a directory on any platform.
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();

        let chosen =
            portable_data_dir(&file.join("under-a-file")).expect("an unwritable root got nothing");

        assert_eq!(chosen, env.home().join(".numera"));
        assert!(
            !chosen.starts_with(root.path()),
            "data was left inside the install the shell cannot write"
        );
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// Numera was renamed from Persona, and the engine still reads
    /// `~/.persona` for as long as `~/.numera` does not exist. Naming the
    /// fallback in the shell means naming the same one, or an upgrade from
    /// Persona comes up with an empty profile list beside a full directory.
    #[test]
    fn the_per_user_fallback_keeps_an_existing_persona_install() {
        let env = Env::new("persona");
        let root = Scratch::new("persona-root");
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();
        std::fs::create_dir_all(env.home().join(".persona")).unwrap();

        let chosen =
            portable_data_dir(&file.join("under-a-file")).expect("an unwritable root got nothing");

        assert_eq!(chosen, env.home().join(".persona"));
    }

    /// Neither location works: no path is invented. The engine has a better
    /// error for this than the shell does, and an explicit `--data-dir` would
    /// only take its remaining options away.
    #[test]
    fn nothing_writable_anywhere_leaves_the_choice_to_the_engine() {
        let env = Env::new("nowhere");
        let root = Scratch::new("nowhere-root");
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();
        let fake_home = env.home().join("not a directory either");
        std::fs::write(&fake_home, b"nor is this").unwrap();
        // SAFETY: `Env` holds the lock that serialises these cases.
        unsafe { std::env::set_var(HOME_VAR, &fake_home) };

        assert!(
            portable_data_dir(&file.join("under-a-file")).is_none(),
            "the shell invented a data directory that nothing can write"
        );
    }

    #[test]
    fn free_port_hands_out_a_bindable_port() {
        let port = free_port().unwrap();
        assert!(port > 0);
        TcpListener::bind(("127.0.0.1", port)).expect("the port we handed out was not free");
    }
}
