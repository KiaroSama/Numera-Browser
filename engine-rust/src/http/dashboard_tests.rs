use super::*;
use crate::common;
use axum::body::to_bytes;

#[test]
fn dashboard_files_and_spa_fallback_match_original_without_swallowing_api()
-> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("dashboard-response")?;
    // Frozen immutable source HTTP observations. Prepare the same four real
    // build files natively and exercise the actual static/authenticated routers.
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native-http-dashboard/responses.json"
    ))?;
    let inputs: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native-http-dashboard/manifest.json"
    ))?;
    let build = root.path().join("source/web");
    std::fs::create_dir_all(build.join("assets"))?;
    let files = inputs["inputs"]
        .as_array()
        .ok_or("Dashboard inputs missing")?;
    assert_eq!(files.len(), 4, "Dashboard input files missing");
    for input in files {
        let bytes: Vec<u8> = serde_json::from_value(input["bytes"].clone())?;
        std::fs::write(
            build.join(
                input["relative"]
                    .as_str()
                    .ok_or("Dashboard input name missing")?,
            ),
            bytes,
        )?;
    }
    let router = mount(Router::new(), Some(&build))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let rows = document["cases"]
            .as_array()
            .ok_or("Dashboard cases missing")?;
        assert_eq!(rows.len(), 14);
        for case in rows {
            let request = axum::http::Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?)
                .body(Body::empty())?;
            let response = router.clone().oneshot(request).await?;
            assert_eq!(
                u16::from(response.status()),
                case["status"].as_u64().ok_or("Status missing")? as u16,
                "{}",
                case["path"]
            );
            assert_eq!(
                response.headers()["content-type"].to_str()?,
                case["type"].as_str().ok_or("Type missing")?
            );
            let body = to_bytes(response.into_body(), 65536).await?;
            let text = String::from_utf8(body.to_vec())?;
            if case["status"] == 200 || case["method"] == "HEAD" {
                assert_eq!(text, case["body"].as_str().ok_or("Body missing")?);
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&text)?,
                    serde_json::from_str::<serde_json::Value>(
                        case["body"].as_str().ok_or("Error body missing")?
                    )?
                );
            }
        }
        let protected = super::super::application::router_with_dashboard(
            root.path().join("native-protected"),
            "fixture-api-token",
            "",
            None,
            "fixture-host",
            super::super::application::SessionApplication {
                installed: Default::default(),
                sessions: std::sync::Arc::new(crate::ownership::registry::Registry::new(
                    root.path().join("native-protected"),
                    "fixture-host".into(),
                    None,
                )?),
            },
            (None, Some(&build)),
        )
        .await?;
        let authenticated = document["authenticated"]
            .as_array()
            .ok_or("Protected dashboard cases missing")?;
        assert_eq!(authenticated.len(), 4);
        for case in authenticated {
            let mut request = axum::http::Request::builder()
                .uri(case["path"].as_str().ok_or("Protected path missing")?);
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api-token");
            }
            let response = protected
                .clone()
                .oneshot(request.body(Body::empty())?)
                .await?;
            assert_eq!(
                u16::from(response.status()),
                case["status"].as_u64().ok_or("Protected status missing")? as u16,
                "{}",
                case["path"]
            );
            assert_eq!(
                response.headers()["content-type"].to_str()?,
                case["type"].as_str().ok_or("Protected type missing")?
            );
            let text = String::from_utf8(to_bytes(response.into_body(), 65536).await?.to_vec())?;
            if case["path"] == "/" {
                assert_eq!(
                    text,
                    case["body"].as_str().ok_or("Protected SPA body missing")?
                );
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&text)?,
                    serde_json::from_str::<serde_json::Value>(
                        case["body"].as_str().ok_or("Protected API body missing")?
                    )?
                );
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    for input in files {
        let expected: Vec<u8> = serde_json::from_value(input["bytes"].clone())?;
        assert_eq!(
            std::fs::read(
                build.join(
                    input["relative"]
                        .as_str()
                        .ok_or("Dashboard input name missing")?
                )
            )?,
            expected,
            "Dashboard request changed a build file",
        );
    }
    assert_eq!(discover(&root.path().join("source")), Some(build));
    root.cleanup()?;
    Ok(())
}
