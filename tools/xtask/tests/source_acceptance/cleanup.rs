use super::*;
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_5_unlink_3ed4e6,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[5-unlink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unlink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(5),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unlink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "5")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_5_rmdir_58af57,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[5-rmdir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("rmdir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(5),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "rmdir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "5")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_5_scandir_c1f058,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[5-scandir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("scandir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(5),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "scandir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "5")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_5_islink_b17cb4,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[5-islink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("islink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(5),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "islink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "5")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_5_unknown_3d80ea,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[5-unknown]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unknown\nprivate-operation"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(5),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unknown")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "5")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_32_unlink_17e5b1,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[32-unlink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unlink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unlink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "32")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_32_rmdir_038ce3,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[32-rmdir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("rmdir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "rmdir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "32")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_32_scandir_571943,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[32-scandir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("scandir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "scandir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "32")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_32_islink_398716,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[32-islink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("islink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "islink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "32")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_32_unknown_b2aae6,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[32-unknown]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unknown\nprivate-operation"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unknown")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "32")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_none_unlink_53d3d0,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[None-unlink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unlink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: None,
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unlink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "None")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_none_rmdir_ee223c,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[None-rmdir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("rmdir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: None,
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "rmdir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "None")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_none_scandir_f31948,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[None-scandir]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("scandir"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: None,
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "scandir")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "None")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_none_islink_ca5a31,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[None-islink]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("islink"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: None,
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "islink")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "None")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_public_run_reports_failed_cleanup_without_swallowing_or_repeating_none_unknown_85f81b,
    "tests/test_migration_cleanup.py::test_public_run_reports_failed_cleanup_without_swallowing_or_repeating[None-unknown]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-input");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("private-fixture-file");
        let outside = t.path().join("outside");
        fs::write(&retained, b"retained fixture bytes")?;
        fs::write(&outside, b"outside bytes")?;
        let mut messages = Vec::new();
        let mut failures = 0;
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    failures += 1;
                    return Err(CleanupFault {
                        operation: CleanupOperation::from_name("unknown\nprivate-operation"),
                        error: io::Error::from_raw_os_error(13),
                        winerror: None,
                    });
                }
                Ok(())
            },
            |d| messages.push(d.to_string()),
        );
        let error = result.err().ok_or("cleanup fault swallowed")?;
        assert_eq!(error.raw_os_error(), Some(13));
        assert_eq!(failures, 1);
        assert_eq!(fs::read(&retained)?, b"retained fixture bytes");
        assert_eq!(fs::read(outside)?, b"outside bytes");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(message.contains(concat!("Owned temp cleanup failed operation=", "unknown")));
        assert!(message.contains("errno=13"));
        assert!(message.contains(concat!("winerror=", "None")));
        assert!(message.contains("kind=file"));
        assert!(message.contains("readonly=false"));
        assert!(message.contains("reparse=false"));
        for forbidden in [
            owned.to_string_lossy().as_ref(),
            retained.to_string_lossy().as_ref(),
            "private-operation",
            "retained fixture bytes",
            "Permission denied",
        ] {
            assert!(!message.contains(forbidden));
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_settled_wall_bfa81b,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[settled-wall]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("settled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !false);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(10),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "true")));
        assert!(expected.contains(concat!("idle_expired=", "false")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if false {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_settled_idle_50c17c,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[settled-idle]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("settled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !false);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(5),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "false")));
        assert!(expected.contains(concat!("idle_expired=", "true")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if false {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_settled_both_dac14a,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[settled-both]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("settled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !false);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(5),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "true")));
        assert!(expected.contains(concat!("idle_expired=", "true")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if false {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_reader_unsettled_wall_bda9cd,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[reader-unsettled-wall]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("unsettled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !true);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(10),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "true")));
        assert!(expected.contains(concat!("idle_expired=", "false")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if true {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_reader_unsettled_idle_cb6509,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[reader-unsettled-idle]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("unsettled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !true);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(5),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "false")));
        assert!(expected.contains(concat!("idle_expired=", "true")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if true {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_timeout_trigger_is_retained_before_fatal_cleanup_reader_unsettled_both_885256,
    "tests/test_migration_cleanup.py::test_timeout_trigger_is_retained_before_fatal_cleanup[reader-unsettled-both]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("retained-input");
        let fixture = Fixture::create(&owned)?;
        fs::write(owned.join("payload"), b"unchanged diagnostic fixture")?;
        let h = helper()?;
        let captured = execute(
            command(&h, "resource-worker").arg(&owned).arg("unsettled"),
            None,
        )?;
        assert_eq!(captured.exit, 0, "{}", captured.stderr);
        let result = ci::decode(captured.stdout.as_bytes())?;
        let evidence: numera_core::ownership::resources::ResourceEvidence =
            serde_json::from_value(result["evidence"].clone())?;
        assert_eq!(evidence.clean(), !true);
        let resources = if result["ok"] == true {
            Ok(evidence)
        } else {
            Err(numera_core::ownership::resources::ResourceFailure::Residual(evidence))
        };
        let timeout = TimeoutObservation {
            elapsed: Duration::from_secs(6),
            silence: Duration::from_secs(6),
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(5),
        };
        let expected = timeout.message();
        assert!(expected.contains(concat!("wall_expired=", "true")));
        assert!(expected.contains(concat!("idle_expired=", "true")));
        let mut messages = Vec::new();
        let result = finish_timeout(fixture, timeout, resources, |message| {
            messages.push(message)
        });
        assert!(result.is_err());
        assert_eq!(messages[0], expected);
        assert_eq!(messages[1], "Owned verification phase=settle-tree");
        assert!(!messages[0].contains(&owned.to_string_lossy().into_owned()));
        assert!(!messages[0].contains("synthetic-command"));
        if true {
            assert!(
                result
                    .err()
                    .ok_or("lost resource refusal")?
                    .to_string()
                    .contains("Native resource cleanup failed")
            );
            assert_eq!(
                fs::read(owned.join("payload"))?,
                b"unchanged diagnostic fixture"
            );
        } else {
            assert_eq!(
                result.err().ok_or("lost timeout")?.to_string(),
                "Migration command exceeded wall/idle budget"
            );
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_cleanup_diagnostic_is_load_bearing_against_exact_previous_runner,
    "tests/test_migration_cleanup.py::test_cleanup_diagnostic_is_load_bearing_against_exact_previous_runner",
    {
        let t = Temp::new()?;
        let owned = t.path().join("historical-native-control");
        let fixture = Fixture::create(&owned)?;
        let retained = owned.join("payload");
        fs::write(&retained, b"retained fixture bytes")?;
        let run = |messages: &mut Vec<String>| -> io::Result<()> {
            remove_owned_with(
                fixture.path(),
                |event, path| {
                    if event == CleanupOperation::Islink && path == retained {
                        return Err(CleanupFault {
                            operation: CleanupOperation::Unlink,
                            error: io::Error::from_raw_os_error(13),
                            winerror: Some(32),
                        });
                    }
                    Ok(())
                },
                |diagnostic| messages.push(diagnostic.to_string()),
            )
        };
        let mut current = Vec::new();
        assert!(run(&mut current).is_err());
        assert_eq!(current.len(), 1);
        assert!(current[0].contains("Owned temp cleanup failed operation=unlink"));
        // The frozen previous runner's defect was a propagated filesystem error with
        // no cleanup diagnostic. Its native counterexample uses the exact same real
        // filesystem failure, discarding only the emitted diagnostic.
        let mut historical = Vec::<String>::new();
        let result = remove_owned_with(
            fixture.path(),
            |event, path| {
                if event == CleanupOperation::Islink && path == retained {
                    return Err(CleanupFault {
                        operation: CleanupOperation::Unlink,
                        error: io::Error::from_raw_os_error(13),
                        winerror: Some(32),
                    });
                }
                Ok(())
            },
            |_| {},
        );
        assert!(result.is_err());
        let detected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_eq!(
                historical.len(),
                1,
                "Owned cleanup failure diagnostic missing"
            );
        }));
        assert!(detected.is_err());
        historical.clear();
        assert_eq!(fs::read(retained)?, b"retained fixture bytes");

        Ok(())
    }
);
