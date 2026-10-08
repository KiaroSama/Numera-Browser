use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::schedule_routes::router;
use serde_json::Value;
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

#[test]
fn schedule_http_routes_preserve_validation_order_and_persisted_rows()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/schedule-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Schedule HTTP cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("schedule-route")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            for (filename, profile) in case["profiles"].as_object().ok_or("Profiles missing")? {
                fs::write(
                    root.join("dashboard").join(filename),
                    serde_json::to_vec(profile)?,
                )?;
            }
            let path = root.join("schedules.json");
            let before = serde_json::to_vec(&case["rows"])?;
            fs::write(&path, &before)?;
            let app = router(root, "fixture-api");
            let start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?)
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&case["input"])?))?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.oneshot(request)).await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let json_body = response
                .headers()
                .get("content-type")
                .is_some_and(|value| value.as_bytes().starts_with(b"application/json"));
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let mut response: Value = if json_body {
                serde_json::from_slice(&bytes)?
            } else {
                Value::String(String::from_utf8(bytes.to_vec())?)
            };
            let bytes = fs::read(path)?;
            let mut written: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(
                Value::Bool(bytes == before),
                case["unchanged"],
                "{}",
                case["name"]
            );
            let end = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let minted = case["method"] == "POST" && case["status"] == 200;
            if minted {
                let id = response["id"]
                    .as_str()
                    .ok_or("Minted ID missing")?
                    .to_owned();
                assert_eq!(id.len(), 12);
                assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                let created = response["createdAt"]
                    .as_f64()
                    .ok_or("Created time missing")?;
                assert!(created >= start && created <= end);
                assert_eq!(response["nextRun"], response["createdAt"]);
                let rows = written.as_array_mut().ok_or("Written rows missing")?;
                let entry = rows.last_mut().ok_or("Added row missing")?;
                assert_eq!(entry["id"], id);
                assert_eq!(entry["createdAt"], created);
                response["id"] = case["body"]["id"].clone();
                response["createdAt"] = case["now"].clone();
                response["nextRun"] = case["now"].clone();
                entry["id"] = case["body"]["id"].clone();
                entry["createdAt"] = case["now"].clone();
            }
            // Source clock is pinned. Assert the actual target clock decision
            // before normalizing that explicitly nondeterministic field.
            let rows = match &mut response {
                Value::Array(rows) => rows.iter_mut().collect::<Vec<_>>(),
                Value::Object(_) if case["status"] == 200 && case["method"] != "DELETE" => {
                    vec![&mut response]
                }
                _ => vec![],
            };
            for row in rows {
                let next = row["nextRun"].as_f64().ok_or("Next run missing")?;
                let due = row["due"].as_bool().ok_or("Due flag missing")?;
                assert!(if due { next <= end } else { next > start });
                row["due"] =
                    Value::Bool(next <= case["now"].as_f64().ok_or("Source time missing")?);
            }
            assert_eq!(response, case["body"], "{}", case["name"]);
            assert_eq!(written, case["written"], "{}", case["name"]);
            temporary.cleanup()?;
        }
        Ok(())
    })
}
