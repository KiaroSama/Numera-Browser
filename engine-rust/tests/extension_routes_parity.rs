use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::extension_routes::router;
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn installed_extension_http_lists_store_and_manual_resources_without_download()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Extension HTTP cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("extension-route")?;
            let root = temporary.path().to_path_buf();
            for (relative, value) in case["files"].as_object().ok_or("Files missing")? {
                let path = root.join(relative);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    value.as_str().ok_or("Fixture text missing")?.as_bytes(),
                )?;
            }
            let defaults = case["defaults"]
                .as_array()
                .ok_or("Defaults missing")?
                .iter()
                .map(|value| {
                    let text = value.as_str().ok_or("Default missing")?;
                    Ok(if text.contains("://") {
                        text.into()
                    } else {
                        root.join(text).to_string_lossy().into_owned()
                    })
                })
                .collect::<Result<Vec<String>, &str>>()?;
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&json!({"default_extensions":defaults}))?,
            )?;
            let app = router(root.clone(), "fixture-api");
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get("/api/extensions/installed")
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
            for entry in body["extensions"]
                .as_array_mut()
                .ok_or("Extension rows missing")?
            {
                for field in ["path", "icon"] {
                    if let Some(path) = entry[field].as_str() {
                        let relative = Path::new(path)
                            .strip_prefix(&root)?
                            .to_string_lossy()
                            .replace('\\', "/");
                        entry[field] = relative.into();
                    }
                }
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            temporary.cleanup()?;
        }
        let temporary = common::TempDirectory::new("extension-install-route")?;
        let root = temporary.path();
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let extension = root.join("extensions").join(id);
        fs::create_dir_all(&extension)?;
        fs::write(
            extension.join("manifest.json"),
            br#"{"name":"Fixture extension","manifest_version":3,"version":"1"}"#,
        )?;
        for add in [true, false] {
            fs::write(root.join("config.json"), b"{}")?;
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                router(root.into(), "fixture-api").oneshot(
                    Request::post("/api/extensions/install")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(
                            &json!({"id":id,"addToDefaults":add}),
                        )?))?,
                ),
            )
            .await??;
            assert_eq!(response.status(), 200);
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let body: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(body["id"], id);
            assert_eq!(body["name"], "Fixture extension");
            assert_eq!(body["reused"], true);
            let expected = if add { json!([extension]) } else { json!([]) };
            assert_eq!(body["default_extensions"], expected);
            let config: Value = serde_json::from_slice(&fs::read(root.join("config.json"))?)?;
            assert_eq!(
                config
                    .get("default_extensions")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
                expected
            );
        }
        temporary.cleanup()?;
        Ok(())
    })
}
