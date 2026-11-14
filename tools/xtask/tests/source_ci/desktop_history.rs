use super::*;
fn proof((damage, platform): (&str, &str)) -> TestResult {
    let temporary = Temp::new("desktop-history")?;
    let path = temporary.write(
        "desktop/src-tauri/src/updates/files.rs",
        b"fixed production bytes\n",
    )?;
    let old = b"historical production bytes\n";
    let mut case = history::desktop_case();
    case.commit = "a".repeat(40);
    case.files.insert(
        "desktop/src-tauri/src/updates/files.rs".into(),
        ci::sha(old),
    );
    let mut inputs = BTreeMap::from([(
        (
            case.commit.clone(),
            "desktop/src-tauri/src/updates/files.rs".into(),
            ci::sha(old),
        ),
        if damage == "hash" {
            b"corrupted historical bytes".to_vec()
        } else {
            old.to_vec()
        },
    )]);
    history::unix_host(
        platform,
        if platform == "linux" {
            "Linux"
        } else {
            "macOS"
        },
        true,
        true,
    )?;
    let result = history::swapped(&temporary.0, &case, &mut inputs, || {
        assert_eq!(fs::read(&path)?, old);
        if damage == "compile" {
            return ci::fail("controlled warm compiler failure");
        }
        if damage == "run" {
            return ci::fail("controlled owned runner failure");
        }
        let mut evidence = evidence::Failure::new(&case.test, &case.symptom);
        for line in failed_transcript(
            &case.test,
            if damage == "wrong-assertion" {
                "unrelated assertion"
            } else {
                &case.symptom
            },
        )
        .lines()
        {
            evidence.accept(line);
        }
        if damage == "restore" {
            fs::remove_file(&path)?;
            fs::create_dir(&path)?;
        }
        evidence.verify(if damage == "zero" { 0 } else { 101 })
    });
    verdict(result, damage == "None");
    if damage == "restore" {
        assert!(
            path.is_dir(),
            "uncertain destination was overwritten during restoration"
        );
    } else {
        assert_eq!(fs::read(&path)?, b"fixed production bytes\n");
    }
    Ok(())
}
source_cases!("tests/test_desktop_history.py::test_desktop_history_requires_semantic_failure_and_fixed_source_restoration",proof,[desktop_proof_good_linux=>("None-linux",("None","linux")),desktop_proof_good_macos=>("None-darwin",("None","darwin")),desktop_proof_compile_linux=>("compile-linux",("compile","linux")),desktop_proof_compile_macos=>("compile-darwin",("compile","darwin")),desktop_proof_run_linux=>("run-linux",("run","linux")),desktop_proof_run_macos=>("run-darwin",("run","darwin")),desktop_proof_zero_linux=>("zero-linux",("zero","linux")),desktop_proof_zero_macos=>("zero-darwin",("zero","darwin")),desktop_proof_wrong_linux=>("wrong-assertion-linux",("wrong-assertion","linux")),desktop_proof_wrong_macos=>("wrong-assertion-darwin",("wrong-assertion","darwin")),desktop_proof_restore_linux=>("restore-linux",("restore","linux")),desktop_proof_restore_macos=>("restore-darwin",("restore","darwin")),desktop_proof_hash_linux=>("hash-linux",("hash","linux")),desktop_proof_hash_macos=>("hash-darwin",("hash","darwin"))]);
fn untrusted(damage: &str) -> TestResult {
    let (mut platform, mut runner, mut trusted, mut clean) = ("linux", "Linux", true, true);
    match damage {
        "windows" => platform = "win32",
        "wrong-runner" => runner = "macOS",
        "untrusted-source" => trusted = false,
        "dirty" => clean = false,
        _ => {}
    }
    assert!(history::unix_host(platform, runner, trusted, clean).is_err());
    Ok(())
}
source_cases!("tests/test_desktop_history.py::test_desktop_history_refuses_before_cargo_on_unverified_source",untrusted,[desktop_untrusted_windows=>("windows","windows"),desktop_untrusted_runner=>("wrong-runner","wrong-runner"),desktop_untrusted_source=>("untrusted-source","untrusted-source"),desktop_untrusted_dirty=>("dirty","dirty")]);
