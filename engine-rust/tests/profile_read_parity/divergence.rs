use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::http::profile_routes::router;
use serde_json::Value;
use std::{fs, time::Duration};
use tower::ServiceExt;

#[test]
fn divergent_profile_http_matches_original_refusal_kind_without_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::common::TempDirectory::new("divergence-http")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/divergent_profile_http_matches_original_refusal_kind_without_mutation.json"
    ))?;
    assert_eq!(cases.len(), 2, "Divergent HTTP observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Divergent inputs missing")?;
        for (name, raw) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
            fs::write(
                path,
                STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
            )?;
        }
        runtime.block_on(async {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                router(root.clone(), "fixture-api", "fixture-host")?.oneshot(
                    Request::post("/api/profiles/fixture00001/launch")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))?,
                ),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "Divergent HTTP status differs: case={index}"
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let actual: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(
                actual, case["body"],
                "Divergent HTTP response kind differs: case={index}"
            );
            for (name, raw) in files {
                assert_eq!(
                    fs::read(root.join(name))?,
                    STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
                    "Native divergent HTTP changed stored bytes"
                );
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
    }
    temporary.cleanup()?;
    Ok(())
}
