use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

fn records(root: &Path) -> Result<Map<String, Value>, Box<dyn std::error::Error>> {
    let mut records = Map::new();
    for directory in ["dashboard", "profiles"] {
        if !root.join(directory).exists() {
            continue;
        }
        for entry in fs::read_dir(root.join(directory))? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                records.insert(
                    format!(
                        "{directory}/{}",
                        path.file_name()
                            .ok_or("Filename missing")?
                            .to_str()
                            .ok_or("Filename not UTF8")?
                    ),
                    serde_json::from_slice(&fs::read(path)?)?,
                );
            }
        }
    }
    Ok(records)
}
fn clock() -> Result<f64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}
fn normalize_clock(
    record: &mut Value,
    expected: &Value,
    field: &str,
    start: f64,
    end: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let actual = record[field].as_f64().ok_or("Stored timestamp missing")?;
    assert!(
        actual >= start && actual <= end,
        "{field}: {actual} outside [{start}, {end}]"
    );
    record[field] = expected[field].clone();
    Ok(())
}

#[test]
fn composed_profile_writes_preserve_source_responses_both_copies_and_config()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-write.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Profile write cases missing")?;
        assert_eq!(cases.len(), 17);
        let mut minted = BTreeSet::new();
        for case in cases {
            let temporary = common::TempDirectory::new("http-profile-write")?;
            let root = temporary.path();
            let initial = case["initial"]
                .as_object()
                .ok_or("Initial copies missing")?;
            for (path, record) in initial {
                let path = root.join(path);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(record)?)?;
            }
            if case["config"]
                .as_object()
                .is_some_and(|config| !config.is_empty())
            {
                fs::write(
                    root.join("config.json"),
                    serde_json::to_vec(&case["config"])?,
                )?;
            }
            let before = initial
                .keys()
                .map(|path| Ok((path.clone(), fs::read(root.join(path))?)))
                .collect::<Result<BTreeMap<_, _>, std::io::Error>>()?;
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
            let start = clock()?;
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::builder()
                        .method(case["method"].as_str().ok_or("Method missing")?)
                        .uri(case["url"].as_str().ok_or("URL missing")?)
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&case["input"])?))?,
                ),
            )
            .await??;
            let end = clock()?;
            let status = response.status().as_u16();
            assert_eq!(
                u64::from(status),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let mut body: Value = serde_json::from_slice(&bytes)?;
            let mut written = records(root)?;
            if status == 200 {
                let create = case["method"] == "POST";
                let actual_id = body["id"].as_str().ok_or("Response ID missing")?.to_owned();
                let expected_id = case["body"]["id"].as_str().ok_or("Source ID missing")?;
                if create {
                    assert_eq!(actual_id.len(), 12);
                    assert!(
                        actual_id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    );
                    assert_ne!(actual_id, "foreign");
                    assert!(minted.insert(actual_id.clone()), "Creation reused an ID");
                    normalize_clock(&mut body, &case["body"], "_created", start, end)?;
                    body["id"] = expected_id.into();
                } else {
                    assert_eq!(actual_id, "existing");
                }
                for directory in ["dashboard", "profiles"] {
                    let actual_path = format!("{directory}/{actual_id}.json");
                    let expected_path = format!("{directory}/{expected_id}.json");
                    let mut record = written.remove(&actual_path).ok_or("Written copy missing")?;
                    let expected = &case["written"][&expected_path];
                    assert_eq!(record["id"], actual_id);
                    if directory == "profiles" {
                        normalize_clock(&mut record, expected, "created_at", start, end)?;
                        normalize_clock(&mut record, expected, "updated_at", start, end)?;
                    } else if create {
                        assert_eq!(
                            record["_created"].as_f64(),
                            serde_json::from_slice::<Value>(&bytes)?["_created"].as_f64()
                        );
                        normalize_clock(&mut record, expected, "_created", start, end)?;
                    }
                    record["id"] = expected_id.into();
                    written.insert(expected_path, record);
                }
            } else {
                for (path, bytes) in before {
                    assert_eq!(
                        fs::read(root.join(path))?,
                        bytes,
                        "Refusal changed stored bytes"
                    );
                }
            }
            assert_eq!(body, case["body"], "{} response", case["name"]);
            assert_eq!(
                Value::Object(written),
                case["written"],
                "{} both copies",
                case["name"]
            );
            let config: Value = if root.join("config.json").exists() {
                serde_json::from_slice(&fs::read(root.join("config.json"))?)?
            } else {
                Value::Object(Map::new())
            };
            assert_eq!(config, case["after_config"], "{} settings", case["name"]);
            temporary.cleanup()?;
        }
        assert_eq!(minted.len(), 7);
        Ok(())
    })
}
