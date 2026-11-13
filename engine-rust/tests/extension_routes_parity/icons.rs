use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::http::extension_routes::router;
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tower::ServiceExt;

#[test]
fn extension_icon_http_matches_original_known_folder_and_file_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::common::TempDirectory::new("extension-icon-http")?;
    let root = temporary.path().canonicalize()?;
    let mut document: Value = crate::characterization::load(
        include_str!("../fixtures/network-characterization/extension-icon-http.json"),
        &root,
    )?;
    let config = serde_json::to_vec(&document["config"])?;
    document["files"]["config.json"] = Value::String(STANDARD.encode(config));
    for (name, raw) in document["files"].as_object().ok_or("Icon inputs missing")? {
        let path = root.join(name);
        fs::create_dir_all(path.parent().ok_or("Icon input parent missing")?)?;
        fs::write(
            path,
            STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
        )?;
    }
    let cases = document["cases"].as_array().ok_or("Icon cases missing")?;
    assert_eq!(cases.len(), 17, "Extension icon HTTP observations missing");
    let files = document["files"].as_object().ok_or("Icon inputs missing")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let app = router(root.clone(), "fixture-api");
        for case in cases {
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["uri"].as_str().ok_or("URI missing")?);
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api");
            }
            for (name, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request =
                    request.header(name.as_str(), value.as_str().ok_or("Header value missing")?);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "Extension icon HTTP status differs: {}",
                case["label"]
            );
            let headers = response.headers().clone();
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let body = if case["method"] != "HEAD"
                && headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with("application/json"))
            {
                serde_json::from_slice(&bytes)?
            } else {
                Value::String(STANDARD.encode(&bytes))
            };
            assert_eq!(
                body, case["body"],
                "Extension icon HTTP body differs: {}",
                case["label"]
            );
            let actual = [
                "content-type",
                "content-length",
                "content-range",
                "accept-ranges",
            ]
            .into_iter()
            .filter_map(|name| headers.get(name).map(|value| (name, value)))
            .map(|(name, value)| Ok((name.to_owned(), Value::String(value.to_str()?.to_owned()))))
            .collect::<Result<serde_json::Map<String, Value>, axum::http::header::ToStrError>>()?;
            assert_eq!(
                json!(actual),
                case["response_headers"],
                "Extension icon HTTP headers differ: {}",
                case["label"]
            );
            for (name, raw) in files {
                assert_eq!(
                    fs::read(root.join(name))?,
                    STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
                    "Native icon request changed fixture bytes"
                );
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}
