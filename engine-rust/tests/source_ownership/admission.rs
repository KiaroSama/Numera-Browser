use super::*;

fn source_runtime(root: &Path) -> io::Result<numera_core::browser::runtime::Runtime> {
    Ok(numera_core::browser::runtime::Runtime {
        adapter: fixture()?,
        cwd: root.into(),
        home: root.into(),
        logs: root.join("logs"),
        country: Some("US".into()),
        relay: None,
        binary_options: Default::default(),
        registry: None,
        deployment: None,
    })
}

source_test!(
    startup_handshake_the_engine_is_read_from_the_profile_the_lease_re_read,
    "tests/test_startup_handshake.py::test_the_engine_is_read_from_the_profile_the_lease_re_read",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Switch(AtomicBool);
        impl Backend for Switch {
            fn admitted(&self, root: &Path, id: &str) -> Option<io::Result<()>> {
                if self.0.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(
                    numera_core::storage::repo::ProfileRepo::new(root.into(), None)
                        .edit(
                            id,
                            |profile| {
                                profile.engine = "camoufox".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other),
                )
            }
        }
        let t = temp("engine-after-session-lease")?;
        let candidate = profile(t.path(), "p")?;
        assert_eq!(candidate.engine, "playwright");
        let runtime = source_runtime(t.path())?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let config = ConfigStore::new(t.path().into());
        let installed = Default::default();
        let cli = numera_core::cli::profile_commands::Profiles {
            repo: &repo,
            config: &config,
            installed: &installed,
            host: "source-host",
        };
        let observer = Arc::new(Switch(AtomicBool::new(false)));
        let answer = with_backend(observer.clone(), || {
            cli.launch(
                &runtime,
                numera_core::cli::launch_commands::LaunchOptions {
                    reference: "p",
                    engine: None,
                    headless: true,
                    time: 1002.0,
                },
                || true,
            )
        })?;
        assert!(
            observer.0.load(Ordering::SeqCst),
            "controlled post-lease edit did not run"
        );
        assert_eq!(repo.get("p")?.ok_or("profile missing")?.engine, "camoufox");
        assert_eq!(answer.payload["engine"], "camoufox");
        assert_eq!(answer.exit, 0);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    startup_handshake_an_explicit_engine_still_wins,
    "tests/test_startup_handshake.py::test_an_explicit_engine_still_wins",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Switch(AtomicBool);
        impl Backend for Switch {
            fn admitted(&self, root: &Path, id: &str) -> Option<io::Result<()>> {
                if self.0.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(
                    numera_core::storage::repo::ProfileRepo::new(root.into(), None)
                        .edit(
                            id,
                            |profile| {
                                profile.engine = "camoufox".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other),
                )
            }
        }
        let t = temp("engine-after-session-lease")?;
        let candidate = profile(t.path(), "p")?;
        assert_eq!(candidate.engine, "playwright");
        let runtime = source_runtime(t.path())?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let config = ConfigStore::new(t.path().into());
        let installed = Default::default();
        let cli = numera_core::cli::profile_commands::Profiles {
            repo: &repo,
            config: &config,
            installed: &installed,
            host: "source-host",
        };
        let observer = Arc::new(Switch(AtomicBool::new(false)));
        let answer = with_backend(observer.clone(), || {
            cli.launch(
                &runtime,
                numera_core::cli::launch_commands::LaunchOptions {
                    reference: "p",
                    engine: Some("playwright"),
                    headless: true,
                    time: 1002.0,
                },
                || true,
            )
        })?;
        assert!(
            observer.0.load(Ordering::SeqCst),
            "controlled post-lease edit did not run"
        );
        assert_eq!(repo.get("p")?.ok_or("profile missing")?.engine, "camoufox");
        assert_eq!(answer.payload["engine"], "playwright");
        assert_eq!(answer.exit, 0);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    startup_handshake_materialising_cannot_revert_a_concurrent_edit,
    "tests/test_startup_handshake.py::test_materialising_cannot_revert_a_concurrent_edit",
    {
        use numera_core::ownership::registry::Registry;
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Edit(AtomicBool);
        impl Backend for Edit {
            fn admitted(&self, root: &Path, id: &str) -> Option<io::Result<()>> {
                if self.0.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(
                    numera_core::storage::repo::ProfileRepo::new(root.into(), None)
                        .edit(
                            id,
                            |profile| {
                                profile.name = "after".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other),
                )
            }
        }
        let t = temp("no-stale-materialization")?;
        let before = profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let changed = Arc::new(Edit(AtomicBool::new(false)));
        let observed = std::cell::Cell::new(false);
        with_backend(changed.clone(), || {
            registry.open_browser("p", "p", |session, profile| {
                assert_eq!(profile.name, "after");
                observed.set(true);
                super::sessions::activate_adapter(session, t.path(), &profile.engine)
            })
        })?;
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(changed.0.load(Ordering::SeqCst));
        assert!(observed.get());
        assert_eq!(before.name, "p");
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert_eq!(repo.get("p")?.ok_or("engine copy absent")?.name, "after");
        let dashboard = numera_core::storage::dashboard::DashboardStore::new(t.path().into(), None);
        assert_eq!(
            dashboard.get("p")?.ok_or("dashboard copy absent")?["name"],
            "after"
        );
        Ok(())
    }
);
source_test!(
    startup_handshake_a_probe_does_not_materialise_a_stale_snapshot,
    "tests/test_startup_handshake.py::test_a_probe_does_not_materialise_a_stale_snapshot",
    {
        use numera_core::ownership::registry::Registry;
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Edit(AtomicBool);
        impl Backend for Edit {
            fn admitted(&self, root: &Path, id: &str) -> Option<io::Result<()>> {
                if self.0.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(
                    numera_core::storage::repo::ProfileRepo::new(root.into(), None)
                        .edit(
                            id,
                            |profile| {
                                profile.name = "after".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other),
                )
            }
        }
        let t = temp("no-stale-materialization")?;
        let before = profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let changed = Arc::new(Edit(AtomicBool::new(false)));
        let observed = std::cell::Cell::new(false);
        with_backend(changed.clone(), || {
            registry.with_finite_browser(
                "p",
                "p",
                |session, profile| {
                    assert_eq!(profile.name, "after");
                    observed.set(true);
                    super::sessions::activate_adapter(session, t.path(), &profile.engine)
                },
                |adapter| {
                    let value =
                        adapter.call("evaluate", Default::default(), Duration::from_secs(1))?;
                    assert_eq!(value["evaluated"], true);
                    Ok(())
                },
            )
        })?;
        assert!(changed.0.load(Ordering::SeqCst));
        assert!(observed.get());
        assert_eq!(before.name, "p");
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert_eq!(repo.get("p")?.ok_or("engine copy absent")?.name, "after");
        let dashboard = numera_core::storage::dashboard::DashboardStore::new(t.path().into(), None);
        assert_eq!(
            dashboard.get("p")?.ok_or("dashboard copy absent")?["name"],
            "after"
        );
        Ok(())
    }
);
source_test!(
    startup_handshake_the_scheduler_does_not_materialise_a_stale_snapshot,
    "tests/test_startup_handshake.py::test_the_scheduler_does_not_materialise_a_stale_snapshot",
    {
        use numera_core::ownership::registry::{BackgroundOptions, Kind, Registry};
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Edit(AtomicBool);
        impl Backend for Edit {
            fn admitted(&self, root: &Path, id: &str) -> Option<io::Result<()>> {
                if self.0.swap(true, Ordering::SeqCst) {
                    return None;
                }
                Some(
                    numera_core::storage::repo::ProfileRepo::new(root.into(), None)
                        .edit(
                            id,
                            |profile| {
                                profile.name = "after".into();
                                Ok(None)
                            },
                            1001.0,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other),
                )
            }
        }
        let t = temp("no-stale-materialization")?;
        let before = profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let changed = Arc::new(Edit(AtomicBool::new(false)));
        let observed = std::cell::Cell::new(false);
        with_backend(changed.clone(), || {
            registry.start_background_checked(
                BackgroundOptions {
                    id: "p",
                    requested_name: "p",
                    kind: Kind::ScheduledWarmup,
                    schedule: Some("source".into()),
                },
                |session, profile| {
                    assert_eq!(profile.name, "after");
                    observed.set(true);
                    super::sessions::activate_adapter(session, t.path(), &profile.engine)
                },
                |_, _| Ok(()),
                || Ok(()),
            )
        })?;
        assert!(registry.shutdown(Duration::from_secs(2)).is_empty());
        assert!(changed.0.load(Ordering::SeqCst));
        assert!(observed.get());
        assert_eq!(before.name, "p");
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert_eq!(repo.get("p")?.ok_or("engine copy absent")?.name, "after");
        let dashboard = numera_core::storage::dashboard::DashboardStore::new(t.path().into(), None);
        assert_eq!(
            dashboard.get("p")?.ok_or("dashboard copy absent")?["name"],
            "after"
        );
        Ok(())
    }
);
