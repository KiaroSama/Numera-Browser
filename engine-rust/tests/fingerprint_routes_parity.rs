use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::fingerprint_routes::router;
use serde_json::Value;
use std::time::Duration;
use tower::ServiceExt;
mod common;

#[test]
fn public_dashboard_coherence_matches_original_ordered_issues_and_http()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::conversion::dashboard_coherence;

    let temporary = common::TempDirectory::new("dashboard-coherence")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/public_dashboard_coherence_matches_original_ordered_issues_and_http.json"
    ))?;
    assert_eq!(cases.len(), 30, "Public coherence observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let app = router(temporary.path().join("native"), "fixture-api");
        for case in &cases {
            let input = case["input"].clone();
            assert_eq!(
                serde_json::to_value(dashboard_coherence(&input, 1700000000.0))?,
                case["issues"],
                "Public dashboard coherence differs: {}",
                case["label"]
            );
            assert_eq!(input, case["input"], "Coherence mutated its input");
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(
                    Request::post("/api/fingerprint/validate")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&input)?))?,
                ),
            )
            .await??;
            assert_eq!(
                response.status(),
                200,
                "Coherence HTTP status differs: {}",
                case["label"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                serde_json::json!({"issues":case["issues"]}),
                "Coherence HTTP result differs: {}",
                case["label"]
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fingerprint_http_validation_uses_original_tolerant_conversion()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/fingerprint-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Fingerprint HTTP cases missing")?;
        assert!(!cases.is_empty());
        let temporary = common::TempDirectory::new("fingerprint-route")?;
        let app = router(temporary.path().to_path_buf(), "fixture-api");
        for case in cases {
            let request = Request::post("/api/fingerprint/validate")
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&case["input"])?))?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(request))
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
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                case["body"],
                "{}",
                case["name"]
            );
        }
        temporary.cleanup()?;
        Ok(())
    })
}
