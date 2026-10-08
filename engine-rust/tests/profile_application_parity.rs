use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

#[test]
fn composed_application_preserves_original_profile_read_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-read.json"))?;
        let cases = fixture["cases"].as_array().ok_or("Profile cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("composed-profile-read")?;
            let root = temporary.path().to_path_buf();
            for (field, directory) in [("profiles", "dashboard"), ("engines", "profiles")] {
                fs::create_dir(root.join(directory))?;
                for (name, record) in case[field].as_object().ok_or("Profile records missing")? {
                    let bytes = match record.as_str() {
                        Some(text) => text.as_bytes().to_vec(),
                        None => serde_json::to_vec(record)?,
                    };
                    fs::write(root.join(directory).join(name), bytes)?;
                }
            }
            if let Some(claim) = case["claim"].as_str() {
                fs::create_dir(root.join("sessions"))?;
                fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
            }
            let app = tokio::time::timeout(
                Duration::from_secs(5),
                router(
                    root.clone(),
                    "fixture-api",
                    "",
                    None,
                    "fixture-host",
                    BTreeMap::new(),
                ),
            )
            .await??;
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
                case["status"].as_u64().ok_or("Status missing")?
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let mut body: Value = serde_json::from_slice(&bytes)?;
            if case["name"] == "engine-adopt" {
                let ended = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
                let created = body[0]["_created"]
                    .as_f64()
                    .ok_or("Adoption timestamp missing")?;
                assert!(created >= started && created <= ended);
                body[0]["_created"] = case["body"][0]["_created"].clone();
            }
            if case["name"] == "last-active" {
                assert!(
                    body[0]["lastActive"]
                        .as_str()
                        .is_some_and(|label| label.ends_with("d ago"))
                );
                body[0]["lastActive"] = case["body"][0]["lastActive"].clone();
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            temporary.cleanup()?;
        }
        Ok(())
    })
}
