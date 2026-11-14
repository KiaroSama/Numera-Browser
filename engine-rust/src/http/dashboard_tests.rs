use super::*;
use crate::common;
use axum::body::to_bytes;

fn source_content_type(case: &serde_json::Value, build: &Path) -> String {
    let frozen = case["type"].as_str().expect("Source content type");
    #[cfg(windows)]
    if case["status"] == 200
        && !case["path"]
            .as_str()
            .expect("Source path")
            .starts_with("/api/")
    {
        let candidate = build.join(
            case["path"]
                .as_str()
                .expect("Source path")
                .trim_start_matches('/'),
        );
        let path = if candidate.is_file() {
            candidate
        } else {
            build.join("index.html")
        };
        if let Some(mut value) = source_registry_type(&path) {
            if value.starts_with("text/") && !value.to_ascii_lowercase().contains("charset=") {
                value.push_str("; charset=utf-8");
            }
            return value;
        }
    }
    #[cfg(not(windows))]
    let _ = build;
    frozen.to_owned()
}

/// The immutable observations were captured on Linux. CPython's Windows
/// mimetypes also reads the local extension registry. Query that source with
/// separate open/query APIs, without changing either the registry or fixture.
#[cfg(windows)]
fn source_registry_type(path: &Path) -> Option<String> {
    use windows_sys::Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY_CLASSES_ROOT, KEY_QUERY_VALUE, REG_SZ, RegCloseKey, RegOpenKeyExW,
            RegQueryValueExW,
        },
    };
    let extension = path.extension()?.to_str()?;
    let key = format!(".{extension}")
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();
    let name = "Content Type".encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut handle = std::ptr::null_mut();
    // SAFETY: terminated names, predefined read-only root and writable handle.
    #[allow(unsafe_code)]
    if unsafe {
        RegOpenKeyExW(
            HKEY_CLASSES_ROOT,
            key.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut handle,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut bytes = std::mem::size_of_val(&buffer) as u32;
    let mut kind = 0;
    // SAFETY: successful open owns this handle; initialized buffer has its exact
    // byte capacity. Close it before interpreting any returned data.
    #[allow(unsafe_code)]
    let status = unsafe {
        let status = RegQueryValueExW(
            handle,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        );
        RegCloseKey(handle);
        status
    };
    if status != ERROR_SUCCESS
        || kind != REG_SZ
        || bytes == 0
        || bytes as usize > std::mem::size_of_val(&buffer)
        || !bytes.is_multiple_of(2)
    {
        return None;
    }
    let units = &buffer[..bytes as usize / 2];
    let end = units.iter().position(|unit| *unit == 0)?;
    let value = String::from_utf16(&units[..end]).ok()?;
    (!value.is_empty() && axum::http::HeaderValue::from_str(&value).is_ok()).then_some(value)
}

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
                source_content_type(case, &build)
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
                source_content_type(case, &build)
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
