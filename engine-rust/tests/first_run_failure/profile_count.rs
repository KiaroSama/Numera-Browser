use numera_core::storage::first_run::run_if_needed;
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn advisory_profile_count_matches_original_when_destination_scan_is_denied()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("import profile count")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = super::support::load(
        include_str!(
            "../fixtures/characterization/advisory_profile_count_matches_original_when_destination_scan_is_denied.json"
        ),
        &root,
    )?;
    assert_eq!(rows.len(), 4, "Advisory profile-count observations missing");
    for (index, row) in rows.iter().enumerate() {
        let case = root.join(index.to_string());
        let source = case.join("source 日本語");
        let destination = case.join("destination with spaces");
        let profiles = destination.join("profiles");
        let denied = row["denied"].as_bool().ok_or("Denial missing")?;
        let populated = row["populated"].as_bool().ok_or("Input missing")?;
        fs::create_dir_all(source.join("profiles"))?;
        fs::create_dir_all(&profiles)?;
        fs::write(source.join("schedules.json"), b"independent")?;
        if populated {
            fs::write(source.join("profiles/a.json"), b"profile\x00\xff")?;
            fs::write(profiles.join("kept.json"), b"destination wins")?;
        }
        if denied {
            fs::set_permissions(&profiles, fs::Permissions::from_mode(0o300))?;
        }
        let scan = fs::read_dir(&profiles);
        let first = run_if_needed(&destination, std::slice::from_ref(&source));
        // Restore the owned directory before errors, assertions or recursive cleanup.
        fs::set_permissions(&profiles, fs::Permissions::from_mode(0o700))?;
        if denied {
            assert_eq!(
                scan.err().map(|error| error.kind()),
                Some(std::io::ErrorKind::PermissionDenied)
            );
        } else {
            drop(scan?);
        }
        for base in [&source, &destination] {
            assert_eq!(fs::read(base.join("schedules.json"))?, b"independent");
            if populated {
                assert_eq!(fs::read(base.join("profiles/a.json"))?, b"profile\x00\xff");
            }
        }
        if populated {
            assert_eq!(fs::read(profiles.join("kept.json"))?, b"destination wins");
        }
        let first = match first {
            Ok(result) => result,
            Err(error) if denied && error.kind() == std::io::ErrorKind::PermissionDenied => {
                panic!("First-run advisory profile count differs: {error}")
            }
            Err(error) => return Err(format!("First-run observer failed: {error}").into()),
        };
        assert_eq!(
            serde_json::to_value(first)?,
            row["first"],
            "First-run advisory profile count differs case={index}"
        );
        assert_eq!(
            serde_json::to_value(run_if_needed(&destination, std::slice::from_ref(&source))?)?,
            row["again"]
        );
        assert_eq!(
            serde_json::to_value(fs::read(destination.join(".imported"))?)?,
            row["marker"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}
