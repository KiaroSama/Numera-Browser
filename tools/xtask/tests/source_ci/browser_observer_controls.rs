pub(super) struct Host {
    pub mode: String,
    pub clock: u64,
    pub events: Vec<&'static str>,
    pub reads: usize,
    pub sleeps: Vec<u64>,
    pub created: bool,
}
impl Host {
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.into(),
            clock: 0,
            events: Vec::new(),
            reads: 0,
            sleeps: Vec::new(),
            created: false,
        }
    }
}
impl browser::Observer for Host {
    fn register(&mut self) -> TestResult {
        self.register_before(self.clock.saturating_add(2_000_000_000))
    }
    fn register_before(&mut self, deadline: u64) -> TestResult {
        if matches!(
            self.mode.as_str(),
            "registration-consumes-wall" | "registration-late"
        ) {
            self.clock = deadline - u64::from(self.mode == "registration-consumes-wall");
            self.events.push("intent");
            return Ok(());
        }
        let mode = self.mode.clone();
        let reads = &mut self.reads;
        let sleeps = &mut self.sleeps;
        let clock = std::cell::Cell::new(self.clock);
        let result = browser::registration(
            42,
            4200,
            deadline,
            || clock.get(),
            |_| {
                *reads += 1;
                if mode == "absent" {
                    return ci::fail("Observer receipt absent");
                }
                let mut trees = vec![json!({"pid":41,"pidToken":4100})];
                if mode != "missing-self" {
                    trees.push(json!({"pid":42,"pidToken":if mode=="wrong-self"{9999}else{4200}}));
                }
                Ok(Report {
                    trees,
                    complete: mode != "incomplete-before"
                        && !(mode == "pending-self" && *reads == 1),
                    problem: None,
                })
            },
            |duration| {
                sleeps.push(u64::try_from(duration.as_millis())?);
                clock.set(clock.get() + u64::try_from(duration.as_nanos())?);
                Ok(())
            },
            |_| Ok(true),
        );
        self.clock = clock.get();
        result?;
        self.events.push("intent");
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        self.clock
    }
    fn spawn(&mut self) -> TestResult {
        self.events.push("spawn");
        if self.mode == "spawn-refusal" {
            return ci::fail("Synthetic spawn refusal");
        }
        self.created = true;
        Ok(())
    }
    fn owned(&self) -> bool {
        self.created
    }
    fn communicate(&mut self, _: u64) -> ci::Result<Value> {
        self.events.push("capture");
        self.clock += 7;
        Ok(json!({"ok":true}))
    }
    fn natural(&mut self, deadline: u64) -> ci::Result<bool> {
        assert!(deadline <= 90_000_000_000);
        self.events.extend(["direct-proof", "sdk-proof"]);
        if self.mode == "incomplete" {
            return ci::fail("Nested receipt incomplete");
        }
        Ok(self.mode != "live")
    }
    fn pause(&mut self) -> TestResult {
        self.sleeps.push(1);
        self.clock += 1_000_000_000;
        Ok(())
    }
    fn peak(&mut self) -> ci::Result<(String, u64)> {
        self.events.push("peak");
        Ok(("fixture".into(), 1234))
    }
    fn diagnostic(&mut self) -> ci::Result<Option<Value>> {
        if self.mode == "diagnostics" {
            self.events.push("snapshot");
            Ok(Some(json!({"coverage":"sampled-not-lifetime-attribution"})))
        } else {
            Ok(None)
        }
    }
    fn cleanup(&mut self) -> TestResult {
        self.events.extend(["direct-cleanup", "sdk-cleanup"]);
        if self.mode == "cleanup" {
            return ci::fail("Nested cleanup refused");
        }
        Ok(())
    }
    fn reap(&mut self) -> TestResult {
        self.events.push("reap");
        if self.mode == "wait" {
            return ci::fail("Leader reap failure");
        }
        Ok(())
    }
    fn close(&mut self) -> TestResult {
        self.events
            .extend(["stdout-close", "stderr-close", "process-close", "job-close"]);
        Ok(())
    }
}
fn lifecycle(mode: &str) -> TestResult {
    let mut host = Host::new(mode);
    let result = browser::observe(&mut host, Duration::from_secs(90));
    if matches!(mode, "normal" | "pending-self" | "diagnostics") {
        let result = result?;
        assert_eq!(result.payload, json!({"ok":true}));
        assert_eq!(result.elapsed_ns, 7);
        assert_eq!(result.bytes, 1234);
        assert_eq!(result.diagnostics.is_some(), mode == "diagnostics");
        let pos = |e| host.events.iter().position(|v| *v == e).expect("event");
        assert!(pos("sdk-proof") < pos("peak") && pos("peak") < pos("sdk-cleanup"));
        assert_eq!(
            host.sleeps,
            if mode == "pending-self" {
                vec![10]
            } else {
                vec![]
            }
        );
    } else {
        assert!(result.is_err());
        if !matches!(mode, "cleanup" | "wait") {
            assert!(!host.events.contains(&"peak"));
        }
    }
    if matches!(
        mode,
        "absent" | "missing-self" | "wrong-self" | "incomplete-before"
    ) {
        assert!(!host.events.contains(&"spawn"));
    } else {
        for event in [
            "direct-cleanup",
            "sdk-cleanup",
            "stdout-close",
            "stderr-close",
        ] {
            assert_eq!(host.events.iter().filter(|v| **v == event).count(), 1);
        }
        assert_eq!(
            &host.events[host.events.len() - 2..],
            ["process-close", "job-close"]
        );
    }
    if mode == "pending-self" {
        parent_publication("delayed")?;
    }
    if mode == "incomplete-before" {
        for publication in ["held-lock", "malformed", "expired"] {
            parent_publication(publication)?;
        }
    }
    if mode == "wrong-self" {
        parent_publication("wrong-birth")?;
    }
    registration_deadline_controls(mode)?;
    Ok(())
}
fn registration_deadline_controls(mode: &str) -> TestResult {
    use std::cell::Cell;
    if mode == "incomplete-before" {
        let mut late = Host::new("registration-late");
        assert!(browser::observe(&mut late, Duration::from_nanos(50)).is_err());
        assert!(!late.events.contains(&"spawn"));
        let mut consumed = Host::new("registration-consumes-wall");
        assert!(browser::observe(&mut consumed, Duration::from_nanos(50)).is_err());
        assert!(consumed.events.contains(&"spawn"));
        assert!(!consumed.events.contains(&"peak"));
        assert!(consumed.events.contains(&"sdk-cleanup"));
    }
    let cases: &[&str] = match mode {
        "pending-self" => &["many-pending", "complete-without-self", "prior-reused-pid"],
        "missing-self" => &["missing", "short-wall"],
        "wrong-self" => &["wrong-birth", "wrong-tree", "verification-error"],
        "incomplete-before" => &["expired", "late-read", "late-tree"],
        "absent" => &["read-error"],
        _ => &[],
    };
    for &case in cases {
        let clock = Cell::new(0u64);
        let reads = Cell::new(0usize);
        let verified = Cell::new(0usize);
        let pauses = std::cell::RefCell::new(Vec::new());
        let deadline = match case {
            "expired" => 0,
            "short-wall" => 15_000_000,
            _ => 90_000_000_000,
        };
        let result = browser::registration(
            42,
            4200,
            deadline,
            || clock.get(),
            |end| {
                assert_eq!(end, deadline.min(2_000_000_000));
                reads.set(reads.get() + 1);
                if case == "read-error" {
                    return Err(
                        io::Error::new(io::ErrorKind::PermissionDenied, "Receipt refused").into(),
                    );
                }
                if case == "late-read" {
                    clock.set(end);
                }
                let waiting = match case {
                    "missing" | "short-wall" => true,
                    "many-pending" | "complete-without-self" => reads.get() < 5,
                    _ => false,
                };
                let mut trees = vec![json!({"pid":41,"pidToken":4100})];
                if case == "prior-reused-pid" {
                    trees.push(json!({"pid":42,"pidToken":4199}));
                }
                if !waiting {
                    trees
                        .push(json!({"pid":42,"pidToken":if case=="wrong-birth" {9999}else{4200}}));
                }
                Ok(Report {
                    trees,
                    complete: case != "many-pending" || !waiting,
                    problem: None,
                })
            },
            |duration| {
                let nanoseconds = u64::try_from(duration.as_nanos())?;
                assert!(nanoseconds > 0 && nanoseconds <= 10_000_000);
                pauses.borrow_mut().push(nanoseconds);
                clock.set(clock.get() + nanoseconds);
                Ok(())
            },
            |_| {
                verified.set(verified.get() + 1);
                if case == "verification-error" {
                    return Err(
                        io::Error::new(io::ErrorKind::PermissionDenied, "Tree refused").into(),
                    );
                }
                if case == "late-tree" {
                    clock.set(2_000_000_000);
                }
                Ok(case != "wrong-tree")
            },
        );
        if matches!(
            case,
            "many-pending" | "complete-without-self" | "prior-reused-pid"
        ) {
            let expected = if case == "prior-reused-pid" {
                BTreeSet::from([(41, 4100), (42, 4199), (42, 4200)])
            } else {
                BTreeSet::from([(41, 4100), (42, 4200)])
            };
            assert_eq!(result?, expected);
            assert_eq!(reads.get(), if case == "prior-reused-pid" { 1 } else { 5 });
            assert_eq!(verified.get(), 1);
            assert_eq!(
                clock.get(),
                if case == "prior-reused-pid" {
                    0
                } else {
                    40_000_000
                }
            );
        } else {
            let error = result.expect_err("Invalid or late parent registration was accepted");
            if matches!(case, "read-error" | "verification-error") {
                assert_eq!(
                    error.downcast_ref::<io::Error>().map(io::Error::kind),
                    Some(io::ErrorKind::PermissionDenied)
                );
            }
        }
        match case {
            "missing" => {
                assert_eq!(clock.get(), 2_000_000_000);
                assert_eq!(reads.get(), 200);
            }
            "short-wall" => {
                assert_eq!(&*pauses.borrow(), &[10_000_000, 5_000_000]);
                assert_eq!(clock.get(), 15_000_000);
            }
            "expired" => {
                assert_eq!(reads.get(), 0);
                assert_eq!(verified.get(), 0);
            }
            "wrong-birth" | "late-read" => {
                assert_eq!(reads.get(), 1);
                assert_eq!(verified.get(), 0);
            }
            "wrong-tree" | "late-tree" | "verification-error" => assert_eq!(verified.get(), 1),
            _ => {}
        }
    }
    Ok(())
}
