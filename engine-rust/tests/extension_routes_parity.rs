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
        Ok(())
    })
}
