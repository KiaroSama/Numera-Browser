use super::common;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    http::profile_routes::router, models::Profile, read_view, storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tower::ServiceExt;

#[test]
fn legacy_read_views_match_original_core_http_and_name_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("legacy-read-views")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let observed: Value = serde_json::from_str(include_str!(
        "../fixtures/characterization/legacy_read_views_match_original_core_http_and_name_admission.json"
    ))?;
    let views = observed["views"].as_array().ok_or("Read views missing")?;
    assert_eq!(views.len(), 24, "Legacy read observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for (index, case) in views.iter().enumerate() {
        let record = case["input"].clone();
        assert_eq!(
            read_view::readable(&record),
            case["default"],
            "Core legacy read view differs: {index}"
        );
        assert_eq!(
            read_view::readable_with(&record, &["notes"], &["future"]),
            case["custom"],
            "Custom legacy read view differs: {index}"
        );
        if let Some(value) = record.get("name") {
            assert_eq!(
                read_view::readable_value("name", value),
                case["name"],
                "Legacy name text differs: {index}"
            );
            assert_eq!(
                read_view::readable_value("tags", value),
                case["tags"],
                "Legacy list view differs: {index}"
            );
            let pairs = vec![
                ("a".to_owned(), value.clone()),
                ("b".to_owned(), json!("unchanged")),
            ];
            assert_eq!(
                serde_json::to_value(read_view::readable_names(&pairs))?,
                case["names"],
                "Legacy name pairs differ: {index}"
            );
        }
        assert_eq!(record, case["input"], "Read helper changed its input");
        let root = temporary.path().join(format!("http-{index}"));
        fs::create_dir_all(root.join("dashboard"))?;
        let mut stored = record.clone();
        stored["id"] = json!("aaaaaaaaaaaa");
        let bytes = serde_json::to_vec(&stored)?;
        let path = root.join("dashboard/aaaaaaaaaaaa.json");
        fs::write(&path, &bytes)?;
        let app = router(root, "fixture-api", "fixture-host")?;
        let response = runtime.block_on(async {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get("/api/profiles")
                        .header("authorization", "Bearer fixture-api")
                        .body(Body::empty())?,
                ),
            )
            .await??;
            let status = response.status().as_u16();
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 65536),
            )
            .await??;
            Ok::<_, Box<dyn std::error::Error>>((status, serde_json::from_slice::<Value>(&bytes)?))
        })?;
        assert_eq!(response.0, 200, "Legacy HTTP read refused: {index}");
        let mut expected = case["dashboard"].clone();
        expected["id"] = json!("aaaaaaaaaaaa");
        expected["status"] = json!("stopped");
        expected["lastActive"] = json!("never");
        assert_eq!(
            response.1,
            json!([expected]),
            "Legacy HTTP read view differs: {index}"
        );
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "HTTP read rewrote legacy bytes: {index}"
        );
    }
    let names = observed["names"]
        .as_array()
        .ok_or("Name refusal observations missing")?;
    assert_eq!(names.len(), 16, "Name refusal observations missing");
    for (index, case) in names.iter().enumerate() {
        let root = temporary.path().join(format!("names-{index}"));
        let directory = case["directory"].as_str().ok_or("Directory missing")?;
        assert!(matches!(directory, "profiles" | "dashboard"));
        fs::create_dir_all(root.join(directory))?;
        let path = root.join(directory).join("aaaaaaaaaaaa.json");
        let bytes = serde_json::to_vec(&case["record"])?;
        fs::write(&path, &bytes)?;
        let repo = ProfileRepo::new(root.clone(), None);
        let profile = Profile::from_record(&case["candidate"])?;
        let result = repo.save(profile, 1700000000.0);
        assert!(
            result.is_err(),
            "Legacy stored name admission differs: case={index}"
        );
        assert_eq!(
            result.err().ok_or("Name refusal missing")?.to_string(),
            case["error"].as_str().ok_or("Expected error missing")?,
            "Legacy name refusal differs: {index}"
        );
        assert_eq!(
            fs::read(path)?,
            bytes,
            "Name refusal changed legacy bytes: {index}"
        );
        assert!(!root.join("profiles/bbbbbbbbbbbb.json").exists());
        assert!(!root.join("dashboard/bbbbbbbbbbbb.json").exists());
    }
    temporary.cleanup()?;
    Ok(())
}
