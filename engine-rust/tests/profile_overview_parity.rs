use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::Value;
use std::{collections::BTreeMap, fs, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn collision_groups_and_summary_match_original_occurrences()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{collision, models::Profile};

    let temporary = common::TempDirectory::new("collision-summary")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/collision_groups_and_summary_match_original_occurrences.json"
    ))?;
    assert_eq!(cases.len(), 10, "Collision summary observations missing");
    for case in cases {
        let profiles = case["profiles"]
            .as_array()
            .ok_or("Collision profiles missing")?
            .iter()
            .map(Profile::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            serde_json::json!(collision::find(&profiles)),
            case["groups"],
            "Collision occurrence groups: {}",
            case["name"]
        );
        assert_eq!(
            collision::summary(&profiles),
            case["summary"],
            "Collision occurrence summary: {}",
            case["name"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn profile_health_and_collisions_preserve_source_readonly_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-overview.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Overview cases missing")?;
        assert_eq!(cases.len(), 26);
        for case in cases {
            let temporary = common::TempDirectory::new("profile-overview")?;
            let root = temporary.path();
            fs::create_dir(root.join("dashboard"))?;
            let mut before = BTreeMap::new();
            for profile in case["profiles"].as_array().ok_or("Profiles missing")? {
                let path = root.join("dashboard").join(format!(
                    "{}.json",
                    profile["id"].as_str().ok_or("ID missing")?
                ));
                let bytes = serde_json::to_vec(profile)?;
                fs::write(&path, &bytes)?;
                before.insert(path, bytes);
            }
            let schedule_bytes = serde_json::to_vec(&case["schedules"])?;
            fs::write(root.join("schedules.json"), &schedule_bytes)?;
            if let Some(claim) = case["claim"].as_str() {
                fs::create_dir(root.join("sessions"))?;
                fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
            }
            let app = tokio::time::timeout(
                Duration::from_secs(5),
                router(
                    root.to_path_buf(),
                    "fixture-api",
                    "",
                    None,
                    "fixture-host",
                    BTreeMap::new(),
                ),
            )
            .await??;
            // Full application startup creates the source store directories.
            // Restore this read-only fixture's absent-engine precondition only
            // after proving startup did not materialize an engine record.
            let engine_directory = root.join("profiles");
            assert!(
                fs::read_dir(&engine_directory)?.next().is_none(),
                "Overview startup materialized an engine copy",
            );
            fs::remove_dir(&engine_directory)?;
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get(case["url"].as_str().ok_or("URL missing")?)
                        .header("authorization", "Bearer fixture-api")
                        .body(Body::empty())?,
                ),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 32768),
            )
            .await??;
            let mut body: Value = serde_json::from_slice(&bytes)?;
            if case["name"] == "recent-activity-health" {
                assert!(
                    body["profiles"][0]["lastActive"]
                        .as_str()
                        .is_some_and(|label| label.ends_with("d ago"))
                );
                body["profiles"][0]["lastActive"] =
                    case["body"]["profiles"][0]["lastActive"].clone();
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            assert_eq!(case["unchanged"], true);
            for (path, bytes) in before {
                assert_eq!(fs::read(path)?, bytes);
            }
            assert_eq!(fs::read(root.join("schedules.json"))?, schedule_bytes);
            assert!(
                !root.join("profiles").exists(),
                "Overview materialized an engine copy"
            );
            temporary.cleanup()?;
        }
        Ok(())
    })
}
