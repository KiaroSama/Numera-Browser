use super::*;
use std::cell::{Cell, RefCell};

pub(super) fn current_identity_control() -> TestResult {
    use std::io::Read;
    let root = temp("observer-current-record")?;
    let path = root.path().join("current.json");
    let foreign_marker = root.path().join("foreign");
    let mut foreign_command = Command::new(fixture()?);
    foreign_command.arg("leaf").arg(&foreign_marker);
    let mut foreign = OwnedTree::spawn(&mut foreign_command)?;
    wait_file(&foreign_marker)?;
    let mut command = Command::new(fixture()?);
    command
        .arg("match-current-record")
        .arg(&path)
        .stdout(Stdio::piped());
    let mut tree = OwnedTree::spawn(&mut command)?;
    let mut record = tree.record();
    record["pidToken"] = record["token"].take();
    record["foreignTree"] = foreign.record()["tree"].clone();
    write_json(&path, &record)?;
    let mut bytes = Vec::new();
    tree.child_mut()
        .stdout
        .take()
        .ok_or("Observer fixture output missing")?
        .read_to_end(&mut bytes)?;
    assert!(tree.child_mut().wait()?.success());
    assert_eq!(
        serde_json::from_slice::<Vec<bool>>(&bytes)?,
        [true, false, false, false, false, false]
    );
    tree.settle(Duration::from_secs(2))?;
    assert!(tree.empty()?);
    foreign.settle(Duration::from_secs(2))?;
    assert!(foreign.empty()?);
    Ok(())
}

fn released_tree(root: &Path) -> io::Result<OwnedTree> {
    let mut command = Command::new(fixture()?);
    command.args([
        "tree",
        root.to_str()
            .ok_or_else(|| io::Error::other("Fixture path"))?,
        "release",
    ]);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut tree = OwnedTree::spawn(&mut command)?;
    wait_file(&root.join("descendant"))?;
    tree.child_mut().wait()?;
    assert!(
        !tree.empty()?,
        "A real published leaf must survive its leader"
    );
    Ok(tree)
}

fn release(tree: &mut OwnedTree, root: &Path) -> io::Result<()> {
    fs::write(root.join("release"), b"exit naturally")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !tree.empty()? {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Released leaf did not exit",
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn proof(
    tree: &mut OwnedTree,
    deadline: Instant,
    initial: Option<bool>,
) -> (Option<bool>, &'static str) {
    let owner = RefCell::new(tree);
    capture::refine_natural_settlement_with(
        deadline,
        (initial, "controlled-initial-sample"),
        true,
        || {
            #[cfg(windows)]
            {
                owner.borrow().complete_job_empty()
            }
            #[cfg(not(windows))]
            {
                owner.borrow_mut().empty()
            }
        },
        || Ok(owner.borrow_mut().child_mut().try_wait()?.is_some()),
    )
}

pub(super) fn controls() -> TestResult {
    // This is the same algorithm used by the Windows final observation. Linux
    // supplies its real complete PGID observation and retained child so the
    // lifecycle/race controls run without a fictitious Windows kernel.
    let root = temp("natural-release")?;
    let mut tree = released_tree(root.path())?;
    let initial = Some(tree.empty()?);
    assert_eq!(initial, Some(false));
    release(&mut tree, root.path())?; // Existing diagnostic work may take this interval.
    assert_eq!(
        proof(&mut tree, Instant::now() + Duration::from_secs(1), initial),
        (Some(true), "fresh-empty-proof"),
        "A fresh complete pre-cleanup proof must replace a stale nonempty sample"
    );
    assert!(tree.child_mut().try_wait()?.is_some());

    let persistent = temp("natural-persistent")?;
    let mut live = released_tree(persistent.path())?;
    let final_live = proof(
        &mut live,
        Instant::now() + Duration::from_secs(1),
        Some(false),
    );
    assert_eq!(final_live, (Some(false), "final-nonempty-tree"));
    live.settle(Duration::from_secs(2))?;
    assert!(live.empty()?);
    assert_eq!(
        final_live.0,
        Some(false),
        "Forced cleanup cannot revise the prior verdict"
    );

    let queries = Cell::new(0);
    for (initial, leader, expired) in [
        (None, true, false),
        (Some(false), false, false),
        (Some(false), true, true),
    ] {
        let result = capture::refine_natural_settlement_with(
            if expired {
                Instant::now()
            } else {
                Instant::now() + Duration::from_secs(1)
            },
            (initial, "initial-refusal"),
            leader,
            || {
                queries.set(queries.get() + 1);
                tree.empty()
            },
            || {
                queries.set(queries.get() + 1);
                Ok(true)
            },
        );
        assert_ne!(result.0, Some(true));
    }
    assert_eq!(
        queries.get(),
        0,
        "Unknown, live leader or expired input must not be queried again"
    );

    assert_eq!(
        capture::refine_natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            (Some(false), "initial"),
            true,
            || Err(io::Error::from_raw_os_error(5)),
            || Ok(true)
        ),
        (None, "final-empty-query-failed")
    );
    assert_eq!(
        capture::refine_natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            (Some(false), "initial"),
            true,
            || Ok(true),
            || Err(io::Error::from_raw_os_error(5))
        ),
        (None, "final-leader-query-failed")
    );
    assert_eq!(
        capture::refine_natural_settlement_with(
            Instant::now() + Duration::from_secs(1),
            (Some(false), "initial"),
            true,
            || tree.empty(),
            || Ok(false),
        ),
        (Some(false), "final-leader-live"),
        "Complete tree emptiness cannot replace the separate retained-leader proof"
    );

    let late = temp("natural-after-deadline")?;
    let mut late_tree = released_tree(late.path())?;
    let deadline = Instant::now() + Duration::from_millis(25);
    release(&mut late_tree, late.path())?;
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        proof(&mut late_tree, deadline, Some(false)),
        (Some(false), "original-deadline")
    );

    let deadline = Instant::now() + Duration::from_millis(15);
    let leader_queries = Cell::new(0);
    assert_eq!(
        capture::refine_natural_settlement_with(
            deadline,
            (Some(false), "initial"),
            true,
            || {
                assert!(late_tree.empty()?);
                while Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(true)
            },
            || {
                leader_queries.set(leader_queries.get() + 1);
                Ok(true)
            },
        ),
        (Some(false), "original-deadline")
    );
    assert_eq!(
        leader_queries.get(),
        0,
        "A late first query must not start a second query"
    );
    let deadline = Instant::now() + Duration::from_millis(15);
    assert_eq!(
        capture::refine_natural_settlement_with(
            deadline,
            (Some(false), "initial"),
            true,
            || Ok(true),
            || {
                assert!(late_tree.child_mut().try_wait()?.is_some());
                while Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(true)
            },
        ),
        (Some(false), "original-deadline"),
        "An exited leader observed after the original deadline cannot be accepted"
    );
    #[cfg(windows)]
    capture_controls()?;
    Ok(())
}

#[cfg(windows)]
fn capture_controls() -> TestResult {
    for mode in ["release", "persistent", "expired"] {
        let root = temp("natural-capture")?;
        let mut command = Command::new(fixture()?);
        command.arg("tree").arg(root.path()).arg("release");
        let wall = Duration::from_secs(2);
        let latest = Instant::now() + wall + Duration::from_millis(50);
        let mut initial = None;
        let mut final_sample = None;
        let mut callback_error = None;
        let mut record = None;
        let mut phases = Vec::new();
        let output = capture::run_observed(
            &mut command,
            limit(wall),
            || false,
            |event, tree| match event {
                capture::Observation::BeforeCleanupSample {
                    natural_settlement, ..
                } => {
                    phases.push("sample");
                    initial = Some(natural_settlement);
                    if mode != "persistent" {
                        let result = (|| -> io::Result<()> {
                            fs::write(root.path().join("release"), b"release")?;
                            while !tree.complete_job_empty()? {
                                if Instant::now() >= latest {
                                    return Err(io::Error::other("Natural leaf did not exit"));
                                }
                                thread::sleep(Duration::from_millis(2));
                            }
                            if mode == "expired" {
                                while Instant::now() < latest {
                                    thread::sleep(Duration::from_millis(1));
                                }
                            }
                            Ok(())
                        })();
                        callback_error = result.err();
                    }
                }
                capture::Observation::BeforeCleanup {
                    natural_settlement,
                    record: r,
                    ..
                } => {
                    phases.push("final");
                    final_sample = Some(natural_settlement);
                    record = Some(r);
                }
                _ => {}
            },
        );
        if let Some(error) = callback_error {
            return Err(error.into());
        }
        if mode == "expired" {
            assert_eq!(
                output.err().map(|error| error.kind()),
                Some(io::ErrorKind::TimedOut)
            );
        } else {
            assert_eq!(output?.exit, 0);
        }
        assert_eq!(
            initial,
            Some(Some(false)),
            "The real leaf must be present in the initial sample"
        );
        assert_eq!(final_sample, Some(Some(mode == "release")));
        assert_eq!(phases, ["sample", "final"]);
        assert!(recorded_tree::empty(
            &record.ok_or("Missing final owner record")?
        )?);
    }
    Ok(())
}
