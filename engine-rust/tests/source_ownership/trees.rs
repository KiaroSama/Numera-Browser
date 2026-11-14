use super::*;

source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_8a56c62e,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[truncated json]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"abc\",\"pid\":1",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_f109e370,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a list, not a record]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(t.path().join("sessions/p.json"), "[]")?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_aeb779e8,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a pid that is not a number]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":\"many\",\"tree\":{\"kind\":\"pgid\",\"group\":1}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_73078977,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a tree that is not an object]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":\"nonsense\"}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_bd1c038a,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a kind that is not a string]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":42}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_c516fd5a,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a pgid that is not a number]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"pgid\",\"group\":\"x\"}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_9b889f0c,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a job whose name is a number]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"job\",\"group\":7}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_c97cb40c,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a session id that is not a string]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":17,\"pid\":3,\"tree\":{\"kind\":\"pgid\",\"group\":1}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_0b95bb7e,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[no identity at all]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"what\":\"a browser session\"}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_be5b1f90,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a 0.26-era leader-only claim]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"token\":99,\"tree\":{\"kind\":\"\",\"group\":null}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_702c4b38,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a leader-only claim with no tree key at all]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"token\":99}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_bce51e2f,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a kind this build does not know]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"cgroup\",\"group\":\"/x\"}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_a4bbdae9,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[the other platform's kind]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"job\",\"group\":\"numera-foreign\"}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_634e5411,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[an empty session id]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"\",\"pid\":3,\"tree\":{\"kind\":\"pgid\",\"group\":1}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_0f378ae4,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a token of the wrong shape]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"token\":{\"t\":1},\"tree\":{\"kind\":\"pgid\",\"group\":1}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_34f82ab9,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a pgid that is a boolean]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"pgid\",\"group\":true}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_a_second_open_cdc1a912,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_a_second_open[a job with an empty name]",
    {
        let t = temp("bad-claim-admission")?;
        profile(t.path(), "p")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(
            t.path().join("sessions/p.json"),
            "{\"session\":\"a\",\"pid\":3,\"tree\":{\"kind\":\"job\",\"group\":\"\"}}",
        )?;
        assert_ne!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(claims::refuse_if_busy(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_this_machine_cannot_check_is_not_free,
    "tests/test_session_ownership.py::test_a_claim_this_machine_cannot_check_is_not_free",
    {
        let t = temp("foreign-host-claim")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        write_json(
            &t.path().join("sessions/p.json"),
            &json!({"session":"foreign","pid":std::process::id(),"host":"another-machine","tree":{"kind":"pgid","group":123}}),
        )?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Unknown
        );
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        Ok(())
    }
);
source_test!(
    session_ownership_an_unreadable_claim_says_what_a_person_has_to_do,
    "tests/test_session_ownership.py::test_an_unreadable_claim_says_what_a_person_has_to_do",
    {
        let t = temp("claim-repair-diagnostic")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(t.path().join("sessions/p.json"), b"{")?;
        let claim = claims::read(t.path(), "p");
        let message = claims::describe(&claim, "source-host");
        assert!(message.contains("sessions/<id>.json"));
        assert!(message.contains("cannot be read"));
        assert!(message.contains("by hand"));
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_is_simply_absent_still_opens,
    "tests/test_session_ownership.py::test_a_claim_that_is_simply_absent_still_opens",
    {
        let t = temp("absent-claim-open")?;
        profile(t.path(), "p")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    session_ownership_an_unevaluable_claim_says_it_is_the_record_that_is_wrong,
    "tests/test_session_ownership.py::test_an_unevaluable_claim_says_it_is_the_record_that_is_wrong",
    {
        let t = temp("unevaluable-claim-message")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        write_json(
            &t.path().join("sessions/p.json"),
            &json!({"session":"a","pid":3,"tree":{"kind":"cgroup","group":"/x"}}),
        )?;
        let claim = claims::read(t.path(), "p");
        assert_eq!(claims::state(&claim, "source-host"), claims::State::Unknown);
        let message = claims::describe(&claim, "source-host");
        assert!(message.contains("form this version cannot check"));
        assert!(message.contains("sessions/<id>.json"));
        Ok(())
    }
);
source_test!(
    session_tree_mandatory_the_claim_names_the_new_tree_before_the_launch_returns,
    "tests/test_session_tree_mandatory.py::test_the_claim_names_the_new_tree_before_the_launch_returns",
    {
        let t = temp("published-tree-claim")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        session.spawn(&mut child, "source-browser")?;
        let tree = session.tree_record().ok_or("session tree absent")?;
        let claims::Claim::Record(claim) = claims::read(t.path(), "p") else {
            panic!("claim not published before spawn returned")
        };
        assert_eq!(claim["pid"], tree["pid"]);
        assert_eq!(claim["tree"], tree["tree"]);
        assert_eq!(claim["token"], tree["token"]);
        assert!(
            claim["session"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        session.finish(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_each_session_writes_its_own_identity,
    "tests/test_session_ownership.py::test_each_session_writes_its_own_identity",
    {
        let t = temp("published-tree-claim")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        session.spawn(&mut child, "source-browser")?;
        let tree = session.tree_record().ok_or("session tree absent")?;
        let claims::Claim::Record(claim) = claims::read(t.path(), "p") else {
            panic!("claim not published before spawn returned")
        };
        assert_eq!(claim["pid"], tree["pid"]);
        assert_eq!(claim["tree"], tree["tree"]);
        assert_eq!(claim["token"], tree["token"]);
        assert!(
            claim["session"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        session.finish(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_two_sessions_in_one_host_get_distinct_trees,
    "tests/test_session_ownership.py::test_two_sessions_in_one_host_get_distinct_trees",
    {
        let t = temp("independent-session-trees")?;
        let mut a = Session::admit(t.path(), "a", "source-host")?;
        let mut b = Session::admit(t.path(), "b", "source-host")?;
        let mut first = command("sleep")?;
        first.stdout(Stdio::null()).stderr(Stdio::null());
        let mut second = command("sleep")?;
        second.stdout(Stdio::null()).stderr(Stdio::null());
        a.spawn(&mut first, "a-browser")?;
        b.spawn(&mut second, "b-browser")?;
        let ar = a.tree_record().ok_or("a tree")?;
        let br = b.tree_record().ok_or("b tree")?;
        assert_ne!(ar["tree"], br["tree"]);
        a.finish(Duration::from_secs(2))?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "a"), "source-host"),
            claims::State::Free
        );
        assert_eq!(
            claims::state(&claims::read(t.path(), "b"), "source-host"),
            claims::State::Busy
        );
        b.finish(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_two_profiles_in_one_process_do_not_hold_each_other,
    "tests/test_session_ownership.py::test_two_profiles_in_one_process_do_not_hold_each_other",
    {
        let t = temp("independent-session-trees")?;
        let mut a = Session::admit(t.path(), "a", "source-host")?;
        let mut b = Session::admit(t.path(), "b", "source-host")?;
        let mut first = command("sleep")?;
        first.stdout(Stdio::null()).stderr(Stdio::null());
        let mut second = command("sleep")?;
        second.stdout(Stdio::null()).stderr(Stdio::null());
        a.spawn(&mut first, "a-browser")?;
        b.spawn(&mut second, "b-browser")?;
        let ar = a.tree_record().ok_or("a tree")?;
        let br = b.tree_record().ok_or("b tree")?;
        assert_ne!(ar["tree"], br["tree"]);
        a.finish(Duration::from_secs(2))?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "a"), "source-host"),
            claims::State::Free
        );
        assert_eq!(
            claims::state(&claims::read(t.path(), "b"), "source-host"),
            claims::State::Busy
        );
        b.finish(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_current_claim_is_recoverable_once_its_tree_is_empty,
    "tests/test_session_ownership.py::test_a_current_claim_is_recoverable_once_its_tree_is_empty",
    {
        let t = temp("recover-empty-claim")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let claim = claims::take(t.path(), "p", &tree, "source-host", "browser")?;
        assert!(!claims::clear(t.path(), "p", "source-host")?);
        tree.settle(Duration::from_secs(2))?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Free
        );
        assert!(claims::clear(t.path(), "p", "source-host")?);
        assert!(!claim.is_empty());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_is_not_released_while_its_own_tree_holds_something,
    "tests/test_session_ownership.py::test_a_claim_is_not_released_while_its_own_tree_holds_something",
    {
        let t = temp("live-tree-claim-release")?;
        let mut tree = spawn_tree(t.path(), false)?;
        tree.child_mut().wait()?;
        let claim = claims::take(t.path(), "p", &tree, "source-host", "browser")?;
        assert!(!tree.empty()?);
        assert!(!claims::release(t.path(), "p", &claim, &mut tree)?);
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        tree.settle(Duration::from_secs(2))?;
        assert!(claims::release(t.path(), "p", &claim, &mut tree)?);
        Ok(())
    }
);
source_test!(
    session_ownership_a_failure_before_the_browser_leaves_nothing_owned,
    "tests/test_session_ownership.py::test_a_failure_before_the_browser_leaves_nothing_owned",
    {
        let t = temp("pre-browser-failure")?;
        {
            let session = Session::admit(t.path(), "p", "source-host")?;
            drop(session);
        }
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        drop(lease);
        Ok(())
    }
);
source_test!(
    session_ownership_a_browser_that_was_closed_again_does_not_keep_the_profile,
    "tests/test_session_ownership.py::test_a_browser_that_was_closed_again_does_not_keep_the_profile",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        struct FinalProof {
            armed: Arc<AtomicBool>,
            failures: AtomicUsize,
        }
        impl Backend for FinalProof {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                if self.armed.swap(false, Ordering::SeqCst) {
                    self.failures.fetch_add(1, Ordering::SeqCst);
                    Some(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "controlled process-row disappearance at final claim proof",
                    )))
                } else {
                    None
                }
            }
        }
        let t = temp("closed-browser-profile-free")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        session.spawn(&mut child, "browser")?;
        let armed = Arc::new(AtomicBool::new(false));
        let flag = armed.clone();
        let target = t.path().join("sessions/p.json");
        let hook = Arc::new(FinalProof {
            armed,
            failures: AtomicUsize::new(0),
        });
        numera_core::vault::testing::with_observer(
            move |event| {
                if event.operation == "before-claim-read" && event.path == target {
                    flag.store(true, Ordering::SeqCst);
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || with_backend(hook.clone(), || session.finish(Duration::from_secs(2))),
        )?;
        assert_eq!(
            hook.failures.load(Ordering::SeqCst),
            1,
            "final proof fault was never reached"
        );
        drop(session);
        assert!(Session::admit(t.path(), "p", "source-host").is_ok());
        Ok(())
    }
);
source_test!(
    session_ownership_an_inner_session_does_not_answer_for_the_one_around_it,
    "tests/test_session_ownership.py::test_an_inner_session_does_not_answer_for_the_one_around_it",
    {
        let t = temp("nested-independent-session")?;
        let mut outer = Session::admit(t.path(), "outer", "source-host")?;
        let mut inner = Session::admit(t.path(), "inner", "source-host")?;
        let mut a = command("sleep")?;
        a.stdout(Stdio::null()).stderr(Stdio::null());
        let mut b = command("sleep")?;
        b.stdout(Stdio::null()).stderr(Stdio::null());
        outer.spawn(&mut a, "outer-browser")?;
        inner.spawn(&mut b, "inner-browser")?;
        inner.finish(Duration::from_secs(2))?;
        assert_eq!(
            claims::state(&claims::read(t.path(), "outer"), "source-host"),
            claims::State::Busy
        );
        outer.finish(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    owned_tree_a_spawned_session_owns_what_it_starts,
    "tests/test_owned_tree.py::test_a_spawned_session_owns_what_it_starts",
    {
        let t = temp("native-owned-descendant")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = as_record(&tree);
        assert!(recorded_tree::verified(&record));
        assert!(!tree.empty()?);
        let descendant: u32 = fs::read_to_string(t.path().join("descendant"))?.parse()?;
        assert!(identity::alive(descendant, None)?);
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    }
);
source_test!(
    owned_tree_the_tree_survives_its_leader,
    "tests/test_owned_tree.py::test_the_tree_survives_its_leader",
    {
        let t = temp("native-owned-descendant")?;
        let mut tree = spawn_tree(t.path(), false)?;
        let record = as_record(&tree);
        assert!(recorded_tree::verified(&record));
        assert!(!tree.empty()?);
        let descendant: u32 = fs::read_to_string(t.path().join("descendant"))?.parse()?;
        assert!(identity::alive(descendant, None)?);
        tree.child_mut().wait()?;
        assert!(
            !tree.empty()?,
            "tree lost its live descendant when the leader exited"
        );
        assert!(recorded_tree::owned(&record)?);
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        assert!(recorded_tree::empty(&record)?);
        settlement_policy_controls();
        Ok(())
    }
);

fn settlement_policy_controls() {
    use std::cell::Cell;
    let emptiness = Cell::new(0);
    let membership = Cell::new(0);
    assert_eq!(
        capture::natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            true,
            || {
                emptiness.set(emptiness.get() + 1);
                Ok(emptiness.get() == 2)
            },
            || {
                membership.set(membership.get() + 1);
                Ok(true)
            },
        ),
        Some(true)
    );
    assert_eq!(
        emptiness.get(),
        2,
        "Accounting settlement needs a final empty-tree proof"
    );
    assert_eq!(membership.get(), 1);

    for unknown in [false, true] {
        let empty_queries = Cell::new(0);
        let result = capture::natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            true,
            || {
                empty_queries.set(empty_queries.get() + 1);
                Ok(false)
            },
            || {
                if unknown {
                    Err(io::Error::other("controlled inaccessible member"))
                } else {
                    Ok(false)
                }
            },
        );
        assert_eq!(result, if unknown { None } else { Some(false) });
        assert_eq!(
            empty_queries.get(),
            1,
            "A live or unknown member must forbid any accounting wait"
        );
    }

    let membership = Cell::new(0);
    assert_eq!(
        capture::natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            true,
            || Ok(false),
            || {
                membership.set(membership.get() + 1);
                Ok(membership.get() == 1)
            },
        ),
        Some(false)
    );
    assert_eq!(
        membership.get(),
        2,
        "Every accounting retry must reject a newly live member"
    );

    let started = Instant::now();
    assert_eq!(
        capture::natural_settlement_with(
            started + Duration::from_secs(3),
            true,
            || Ok(false),
            || Ok(true),
        ),
        Some(false),
        "Exited members cannot replace the final empty-Job proof"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "Accounting that never settles must keep its 249ms cap"
    );

    let membership = Cell::new(0);
    assert_eq!(
        capture::natural_settlement_with(
            Instant::now(),
            true,
            || Ok(false),
            || {
                membership.set(membership.get() + 1);
                Ok(true)
            },
        ),
        Some(false)
    );
    assert_eq!(
        membership.get(),
        0,
        "An exhausted caller deadline cannot acquire a new accounting budget"
    );
    assert_eq!(
        capture::natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            false,
            || Ok(false),
            || {
                membership.set(membership.get() + 1);
                Ok(true)
            },
        ),
        Some(false)
    );
    assert_eq!(
        membership.get(),
        0,
        "A running producer cannot enter accounting settlement"
    );
    assert_eq!(
        capture::natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            true,
            || Err(io::Error::other("controlled uncertain Job emptiness")),
            || Ok(true),
        ),
        None
    );
}
source_test!(
    owned_tree_ending_a_session_ends_the_whole_tree,
    "tests/test_owned_tree.py::test_ending_a_session_ends_the_whole_tree",
    {
        let t = temp("native-owned-descendant")?;
        let mut tree = spawn_tree(t.path(), false)?;
        let record = as_record(&tree);
        assert!(recorded_tree::verified(&record));
        assert!(!tree.empty()?);
        let descendant: u32 = fs::read_to_string(t.path().join("descendant"))?.parse()?;
        assert!(identity::alive(descendant, None)?);
        tree.child_mut().wait()?;
        assert!(
            !tree.empty()?,
            "tree lost its live descendant when the leader exited"
        );
        assert!(recorded_tree::owned(&record)?);
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    owned_tree_the_tree_is_not_the_engines_own_process_group,
    "tests/test_owned_tree.py::test_the_tree_is_not_the_engines_own_process_group",
    {
        let t = temp("distinct-kernel-tree")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = tree.record();
        assert_ne!(record["pid"], json!(std::process::id()));
        #[cfg(unix)]
        {
            #[allow(unsafe_code)]
            let own_group = unsafe { libc::getpgrp() };
            assert_ne!(record["tree"]["group"], json!(own_group));
        }
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    owned_tree_an_unverified_tree_is_never_signalled,
    "tests/test_owned_tree.py::test_an_unverified_tree_is_never_signalled",
    {
        let t = temp("unverified-tree-no-signal")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let mut record = as_record(&tree);
        record["pidToken"] = Value::Null;
        record["tree"] = json!({"kind":"","group":null});
        assert!(!recorded_tree::verified(&record));
        assert!(recorded_tree::settle(&record, Duration::from_millis(20)).is_err());
        assert!(!tree.empty()?, "unverified record killed a live child");
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_a_child_that_survives_the_kill_is_not_reported_as_stopped,
    "tests/test_proved_dead.py::test_a_child_that_survives_the_kill_is_not_reported_as_stopped",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Fault {
            forced: AtomicBool,
            calls: std::sync::Mutex<Vec<bool>>,
            escalates: bool,
            refuses: bool,
        }
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(self.escalates && self.forced.load(Ordering::SeqCst)))
            }
            fn signal(&self, _: i32, force: bool) -> Option<io::Result<()>> {
                self.calls.lock().expect("signal log").push(force);
                if force {
                    self.forced.store(true, Ordering::SeqCst);
                }
                Some(if self.refuses {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected signal refusal",
                    ))
                } else {
                    Ok(())
                })
            }
        }
        let t = temp("prove-before-release")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = as_record(&tree);
        let fault = Arc::new(Fault {
            forced: AtomicBool::new(false),
            calls: std::sync::Mutex::new(Vec::new()),
            escalates: false,
            refuses: false,
        });
        let result = with_backend(fault.clone(), || {
            recorded_tree::settle(&record, Duration::from_millis(40))
        });
        let calls = fault.calls.lock().expect("calls").clone();
        assert!(!result?);
        assert_eq!(
            calls,
            if cfg!(windows) {
                vec![true]
            } else {
                vec![false, true]
            }
        );
        assert!(
            !tree.empty()?,
            "fixture cleanup ran before acceptance observation"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_a_signal_that_is_refused_is_not_reported_as_stopped,
    "tests/test_proved_dead.py::test_a_signal_that_is_refused_is_not_reported_as_stopped",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Fault {
            forced: AtomicBool,
            calls: std::sync::Mutex<Vec<bool>>,
            escalates: bool,
            refuses: bool,
        }
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(self.escalates && self.forced.load(Ordering::SeqCst)))
            }
            fn signal(&self, _: i32, force: bool) -> Option<io::Result<()>> {
                self.calls.lock().expect("signal log").push(force);
                if force {
                    self.forced.store(true, Ordering::SeqCst);
                }
                Some(if self.refuses {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected signal refusal",
                    ))
                } else {
                    Ok(())
                })
            }
        }
        let t = temp("prove-before-release")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = as_record(&tree);
        let fault = Arc::new(Fault {
            forced: AtomicBool::new(false),
            calls: std::sync::Mutex::new(Vec::new()),
            escalates: false,
            refuses: true,
        });
        let result = with_backend(fault.clone(), || {
            recorded_tree::settle(&record, Duration::from_millis(40))
        });
        let calls = fault.calls.lock().expect("calls").clone();
        assert!(result.is_err());
        assert_eq!(
            calls,
            if cfg!(windows) {
                vec![true]
            } else {
                vec![false]
            }
        );
        assert!(
            !tree.empty()?,
            "fixture cleanup ran before acceptance observation"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_a_job_that_ignores_the_first_signal_is_escalated,
    "tests/test_proved_dead.py::test_a_job_that_ignores_the_first_signal_is_escalated",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Fault {
            forced: AtomicBool,
            calls: std::sync::Mutex<Vec<bool>>,
            escalates: bool,
            refuses: bool,
        }
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(self.escalates && self.forced.load(Ordering::SeqCst)))
            }
            fn signal(&self, _: i32, force: bool) -> Option<io::Result<()>> {
                self.calls.lock().expect("signal log").push(force);
                if force {
                    self.forced.store(true, Ordering::SeqCst);
                }
                Some(if self.refuses {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected signal refusal",
                    ))
                } else {
                    Ok(())
                })
            }
        }
        let t = temp("prove-before-release")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = as_record(&tree);
        let fault = Arc::new(Fault {
            forced: AtomicBool::new(false),
            calls: std::sync::Mutex::new(Vec::new()),
            escalates: true,
            refuses: false,
        });
        let result = with_backend(fault.clone(), || {
            recorded_tree::settle(&record, Duration::from_millis(40))
        });
        let calls = fault.calls.lock().expect("calls").clone();
        assert!(result?);
        assert_eq!(
            calls,
            if cfg!(windows) {
                vec![true]
            } else {
                vec![false, true]
            }
        );
        assert!(
            !tree.empty()?,
            "fixture cleanup ran before acceptance observation"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_a_job_that_cannot_be_killed_is_not_recorded_as_stopped,
    "tests/test_proved_dead.py::test_a_job_that_cannot_be_killed_is_not_recorded_as_stopped",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Fault {
            forced: AtomicBool,
            calls: std::sync::Mutex<Vec<bool>>,
            escalates: bool,
            refuses: bool,
        }
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(self.escalates && self.forced.load(Ordering::SeqCst)))
            }
            fn signal(&self, _: i32, force: bool) -> Option<io::Result<()>> {
                self.calls.lock().expect("signal log").push(force);
                if force {
                    self.forced.store(true, Ordering::SeqCst);
                }
                Some(if self.refuses {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected signal refusal",
                    ))
                } else {
                    Ok(())
                })
            }
        }
        let t = temp("prove-before-release")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = as_record(&tree);
        let fault = Arc::new(Fault {
            forced: AtomicBool::new(false),
            calls: std::sync::Mutex::new(Vec::new()),
            escalates: false,
            refuses: true,
        });
        fs::create_dir_all(t.path().join("jobs"))?;
        let mut job = record.clone();
        job["id"] = "job".into();
        job["started"] = 1000.0.into();
        write_json(&t.path().join("jobs/job.json"), &job)?;
        let store = numera_core::jobs::JobStore::new(t.path().into());
        let result = with_backend(fault.clone(), || store.stop("job", "source-host", 1001.0));
        assert!(result.is_err());
        assert!(!t.path().join("jobs/job.exit").exists());
        let after = store.get("job", 1001.0)?.ok_or("job disappeared")?;
        assert_ne!(after["state"], "stopped");
        assert_eq!(after["running"], true);
        assert!(
            !tree.empty()?,
            "fixture cleanup ran before acceptance observation"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_claim_that_cannot_be_read_refuses_every_destructive_operation,
    "tests/test_session_ownership.py::test_a_claim_that_cannot_be_read_refuses_every_destructive_operation",
    {
        let t = temp("destructive-claim-guard")?;
        profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let data = repo.user_data_path("p")?;
        fs::write(data.join("Cookies"), b"live")?;
        let archive = t.path().join("before.numera");
        repo.backup("p", &archive, "source-host")?
            .ok_or("backup absent")?;
        fs::create_dir_all(t.path().join("sessions"))?;
        fs::write(t.path().join("sessions/p.json"), b"{")?;
        assert!(repo.delete("p", "source-host", 1001.0).is_err());
        assert!(repo.delete_session("p", "source-host").is_err());
        assert!(
            repo.backup("p", &t.path().join("during.numera"), "source-host")
                .is_err()
        );
        assert!(
            repo.restore(&archive, false, "source-host", 1001.0)
                .is_err()
        );
        assert!(repo.get("p")?.is_some());
        assert_eq!(fs::read(data.join("Cookies"))?, b"live");
        Ok(())
    }
);
source_test!(
    session_ownership_a_destructive_operation_refuses_our_own_standing_claim,
    "tests/test_session_ownership.py::test_a_destructive_operation_refuses_our_own_standing_claim",
    {
        let t = temp("destructive-claim-guard")?;
        profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let data = repo.user_data_path("p")?;
        fs::write(data.join("Cookies"), b"live")?;
        let archive = t.path().join("before.numera");
        repo.backup("p", &archive, "source-host")?
            .ok_or("backup absent")?;
        let mut tree = spawn_tree(t.path(), true)?;
        claims::take(t.path(), "p", &tree, "source-host", "browser")?;
        assert!(repo.delete("p", "source-host", 1001.0).is_err());
        assert!(repo.delete_session("p", "source-host").is_err());
        assert!(
            repo.backup("p", &t.path().join("during.numera"), "source-host")
                .is_err()
        );
        assert!(
            repo.restore(&archive, false, "source-host", 1001.0)
                .is_err()
        );
        assert!(repo.get("p")?.is_some());
        assert_eq!(fs::read(data.join("Cookies"))?, b"live");
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    session_ownership_the_public_session_delete_honours_the_durable_claim,
    "tests/test_session_ownership.py::test_the_public_session_delete_honours_the_durable_claim",
    {
        let t = temp("destructive-claim-guard")?;
        profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let data = repo.user_data_path("p")?;
        fs::write(data.join("Cookies"), b"live")?;
        let archive = t.path().join("before.numera");
        repo.backup("p", &archive, "source-host")?
            .ok_or("backup absent")?;
        let mut tree = spawn_tree(t.path(), true)?;
        claims::take(t.path(), "p", &tree, "source-host", "browser")?;
        assert!(repo.delete("p", "source-host", 1001.0).is_err());
        assert!(repo.delete_session("p", "source-host").is_err());
        assert!(
            repo.backup("p", &t.path().join("during.numera"), "source-host")
                .is_err()
        );
        assert!(
            repo.restore(&archive, false, "source-host", 1001.0)
                .is_err()
        );
        assert!(repo.get("p")?.is_some());
        assert_eq!(fs::read(data.join("Cookies"))?, b"live");
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
#[cfg(target_os = "linux")]
source_test!(posix_observation_errors_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable_3fdba35f,"tests/test_posix_observation_errors.py::test_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable[13]",{
use numera_core::ownership::test_support::{Backend,with_backend};struct Fault{pid:u32}impl Backend for Fault{fn stat(&self,path:&Path)->Option<io::Result<String>>{(path==Path::new("/proc").join(self.pid.to_string()).join("stat")).then(||Err(io::Error::from_raw_os_error(13)))}}
let t=temp("real-child-stat-refusal")?;let mut tree=spawn_tree(t.path(),true)?;let record=tree.record();let result=with_backend(Arc::new(Fault{pid:tree.pid()}),||numera_core::ownership::tree::record_empty(&record));assert!(!matches!(result,Ok(true)));assert!(tree.child_mut().try_wait()?.is_none());tree.settle(Duration::from_secs(2))?;Ok(())
});
#[cfg(target_os = "linux")]
source_test!(posix_observation_errors_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable_ef2d127d,"tests/test_posix_observation_errors.py::test_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable[5]",{
use numera_core::ownership::test_support::{Backend,with_backend};struct Fault{pid:u32}impl Backend for Fault{fn stat(&self,path:&Path)->Option<io::Result<String>>{(path==Path::new("/proc").join(self.pid.to_string()).join("stat")).then(||Err(io::Error::from_raw_os_error(5)))}}
let t=temp("real-child-stat-refusal")?;let mut tree=spawn_tree(t.path(),true)?;let record=tree.record();let result=with_backend(Arc::new(Fault{pid:tree.pid()}),||numera_core::ownership::tree::record_empty(&record));assert!(!matches!(result,Ok(true)));assert!(tree.child_mut().try_wait()?.is_none());tree.settle(Duration::from_secs(2))?;Ok(())
});
#[cfg(target_os = "linux")]
source_test!(
    posix_observation_errors_bad_stat_text_is_unknown_not_a_zombie,
    "tests/test_posix_observation_errors.py::test_bad_stat_text_is_unknown_not_a_zombie",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault {
            pid: u32,
        }
        impl Backend for Fault {
            fn stat(&self, path: &Path) -> Option<io::Result<String>> {
                (path == Path::new("/proc").join(self.pid.to_string()).join("stat"))
                    .then(|| Ok("incomplete stat".into()))
            }
        }
        let t = temp("real-child-stat-malformed")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let record = tree.record();
        let result = with_backend(Arc::new(Fault { pid: tree.pid() }), || {
            numera_core::ownership::tree::record_empty(&record)
        });
        assert!(!matches!(result, Ok(true)));
        assert!(tree.child_mut().try_wait()?.is_none());
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
#[cfg(target_os = "linux")]
source_test!(
    posix_observation_errors_process_table_listing_failure_is_unknown,
    "tests/test_posix_observation_errors.py::test_process_table_listing_failure_is_unknown",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault(PathBuf);
        impl Backend for Fault {
            fn procfs_root(&self) -> Option<PathBuf> {
                Some(self.0.clone())
            }
        }
        let t = temp("process-table-listing-error")?;
        let missing = t.path().join("not-a-directory");
        fs::write(&missing, b"obstruction")?;
        let result = with_backend(Arc::new(Fault(missing)), || {
            numera_core::ownership::tree::record_empty(
                &json!({"tree":{"kind":"pgid","group":123456789}}),
            )
        });
        assert!(result.is_err());
        Ok(())
    }
);
#[cfg(target_os = "linux")]
source_test!(
    posix_observation_errors_disappearing_procfs_entry_is_not_an_observation_error,
    "tests/test_posix_observation_errors.py::test_disappearing_procfs_entry_is_not_an_observation_error",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault(PathBuf);
        impl Backend for Fault {
            fn procfs_root(&self) -> Option<PathBuf> {
                Some(self.0.clone())
            }
        }
        let t = temp("disappeared-process-stat")?;
        fs::create_dir(t.path().join("101"))?;
        let result = with_backend(Arc::new(Fault(t.path().into())), || {
            numera_core::ownership::tree::record_empty(&json!({"tree":{"kind":"pgid","group":123}}))
        });
        assert!(result?);
        Ok(())
    }
);
source_test!(
    process_observation_errors_only_esrch_proves_the_group_gone_a68a6ebf,
    "tests/test_process_observation_errors.py::test_only_esrch_proves_the_group_gone[EINVAL]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault;
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Err(io::Error::from_raw_os_error(22)))
            }
        }
        let t = temp("native-kernel-group-error")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let result = with_backend(Arc::new(Fault), || {
            numera_core::ownership::tree::record_empty(&tree.record())
        });
        assert!(
            result.is_err(),
            "unknown group observation was called empty"
        );
        assert!(
            !tree.empty()?,
            "fixture killed its real group before acceptance"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    process_observation_errors_only_esrch_proves_the_group_gone_fdd16eb6,
    "tests/test_process_observation_errors.py::test_only_esrch_proves_the_group_gone[EIO]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault;
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Err(io::Error::from_raw_os_error(5)))
            }
        }
        let t = temp("native-kernel-group-error")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let result = with_backend(Arc::new(Fault), || {
            numera_core::ownership::tree::record_empty(&tree.record())
        });
        assert!(
            result.is_err(),
            "unknown group observation was called empty"
        );
        assert!(
            !tree.empty()?,
            "fixture killed its real group before acceptance"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    process_observation_errors_only_esrch_proves_the_group_gone_2768196b,
    "tests/test_process_observation_errors.py::test_only_esrch_proves_the_group_gone[ESRCH]",
    {
        let t = temp("healthy-owned-group")?;
        let mut tree = spawn_tree(t.path(), true)?;
        assert!(!tree.empty()?);
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        Ok(())
    }
);
source_test!(
    process_observation_errors_only_esrch_proves_the_group_gone_7ac8d0a2,
    "tests/test_process_observation_errors.py::test_only_esrch_proves_the_group_gone[EPERM]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault;
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Err(io::Error::from_raw_os_error(1)))
            }
        }
        let t = temp("native-kernel-group-error")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let result = with_backend(Arc::new(Fault), || {
            numera_core::ownership::tree::record_empty(&tree.record())
        });
        assert!(
            result.is_err(),
            "unknown group observation was called empty"
        );
        assert!(
            !tree.empty()?,
            "fixture killed its real group before acceptance"
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    process_observation_errors_only_esrch_proves_the_group_gone_86d90d7c,
    "tests/test_process_observation_errors.py::test_only_esrch_proves_the_group_gone[probe-ok]",
    {
        let t = temp("healthy-owned-group")?;
        let mut tree = spawn_tree(t.path(), true)?;
        assert!(!tree.empty()?);
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_fe6fb04d,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[1-123 S 123\\n-True]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("123"))?;
        fs::write(t.path().join("123/stat"), "123 (fixture) S 1 123")?;
        let observed = fixture_row_empty(t.path(), 789, false);
        assert!(!matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_c112a7f1,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-invalid-row\\n-True]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("101"))?;
        fs::write(t.path().join("101/stat"), "incomplete stat")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(!matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_ca03908d,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 Z 123\\n-False]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("123"))?;
        fs::write(t.path().join("123/stat"), "123 (fixture) Z 1 123")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_612fc93e,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 Z+ 123\\n-False]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("123"))?;
        fs::write(t.path().join("123/stat"), "123 (fixture) Z+ 1 123")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_dfe34cc5,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 S 123\\n-True]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("123"))?;
        fs::write(t.path().join("123/stat"), "123 (fixture) S 1 123")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(!matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_3eaa024c,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 S 0\\n-True]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("123"))?;
        fs::write(t.path().join("123/stat"), "123 (fixture) S 1 0")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(!matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_3ecfa790,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-987 S 987\\n-False]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("987"))?;
        fs::write(t.path().join("987/stat"), "987 (fixture) S 1 987")?;
        let observed = fixture_row_empty(t.path(), 123, true);
        assert!(matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_ps_fallback_checks_exit_and_row_integrity_8c22ac29,
    "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0--True]",
    {
        let t = temp("native-process-row-integrity")?;
        fs::create_dir(t.path().join("101"))?;
        fs::write(t.path().join("101/stat"), "incomplete stat")?;
        let observed = fixture_row_empty(t.path(), 123, false);
        assert!(!matches!(observed, Ok(true)));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_probe_disappearance_and_permission_controls_1b2e813d,
    "tests/test_posix_observation_errors.py::test_probe_disappearance_and_permission_controls[3-False]",
    {
        assert!(numera_core::ownership::tree::record_empty(
            &json!({"tree":{"kind":"pgid","group":i32::MAX}})
        )?);
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    posix_observation_errors_probe_disappearance_and_permission_controls_743549ca,
    "tests/test_posix_observation_errors.py::test_probe_disappearance_and_permission_controls[13-True]",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Fault;
        impl Backend for Fault {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Err(io::Error::from_raw_os_error(13)))
            }
        }
        let result = with_backend(Arc::new(Fault), || {
            numera_core::ownership::tree::record_empty(&json!({"tree":{"kind":"pgid","group":123}}))
        });
        assert!(!matches!(result, Ok(true)));
        Ok(())
    }
);
