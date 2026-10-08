use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::profile_routes::router;
use serde_json::Value;
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

#[test]
fn profile_http_read_repairs_only_view_and_reports_uncertain_claim_as_running()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-read.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Profile read cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("profile-read")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            fs::create_dir(root.join("profiles"))?;
            for (filename, profile) in case["engines"]
                .as_object()
                .ok_or("Engine profiles missing")?
            {
                let bytes = match profile.as_str() {
                    Some(text) => text.as_bytes().to_vec(),
                    None => serde_json::to_vec(profile)?,
                };
                fs::write(root.join("profiles").join(filename), bytes)?;
            }
            let profiles = case["profiles"].as_object().ok_or("Profiles missing")?;
            let mut originals = Vec::new();
            for (filename, profile) in profiles {
                let bytes = match profile.as_str() {
                    Some(text) => text.as_bytes().to_vec(),
                    None => serde_json::to_vec(profile)?,
                };
                fs::write(root.join("dashboard").join(filename), &bytes)?;
                originals.push((filename, bytes));
            }
            if let Some(claim) = case["claim"].as_str() {
                fs::create_dir(root.join("sessions"))?;
                fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
            }
            let app = router(root.clone(), "fixture-api", "fixture-host")?;
            let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get("/api/profiles")
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
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let mut body: Value = serde_json::from_slice(&bytes)?;
            if case["name"] == "last-active" {
                let label = body[0]["lastActive"]
                    .as_str()
                    .ok_or("Last active label missing")?;
                assert!(
                    label.ends_with("d ago"),
                    "Actual old timestamp must use days"
                );
                body[0]["lastActive"] = case["body"][0]["lastActive"].clone();
            }
            if case["name"] == "engine-adopt" {
                let ended = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
                assert!(
                    body[0]["_created"]
                        .as_f64()
                        .is_some_and(|time| time >= started && time <= ended),
                    "HTTP adoption creation must be within the request's clock bounds"
                );
                body[0]["_created"] = case["body"][0]["_created"].clone();
                let adopted: Value =
                    serde_json::from_slice(&fs::read(root.join("dashboard/engine-fixture.json"))?)?;
                let mut expected_view = adopted.clone();
                assert_eq!(
                    adopted["_created"],
                    serde_json::from_slice::<Value>(&bytes)?[0]["_created"]
                );
                expected_view["_created"] =
                    case["adopted"]["engine-fixture.json"]["_created"].clone();
                assert_eq!(expected_view, case["adopted"]["engine-fixture.json"]);
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            for (filename, original) in originals {
                assert_eq!(
                    Value::Bool(original == fs::read(root.join("dashboard").join(filename))?),
                    case["unchanged"][filename]
                );
            }
            if case["name"] == "engine-damaged" {
                assert!(fs::read_dir(root.join("dashboard"))?.next().is_none());
            }
            temporary.cleanup()?;
        }
        Ok(())
    })
}
