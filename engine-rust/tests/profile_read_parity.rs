use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::profile_routes::router;
use serde_json::Value;
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;
#[path = "profile_read_parity/read_view.rs"]
mod read_view;

#[test]
fn relative_activity_http_matches_original_without_rewriting_storage()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use serde_json::json;
    use std::{path::Path, process::Command};

    let temporary = common::TempDirectory::new("relative-activity")?;
    let clock = || -> Result<f64, Box<dyn std::error::Error>> {
        Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
    };
    let anchor = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // Integer records retain exact JSON values across both serializers.
    let wide = |number: f64| serde_json::from_str::<Value>(&format!("{number:.0}"));
    let cases = [
        ("true", json!(true)),
        ("false", json!(false)),
        ("null", Value::Null),
        ("zero", json!(0)),
        ("negative-zero", json!(-0.0)),
        ("empty-text", json!("")),
        ("empty-list", json!([])),
        ("empty-object", json!({})),
        ("numeric-text", json!("1")),
        ("text", json!("invalid")),
        ("list", json!([1])),
        ("object", json!({"time": 1})),
        ("future", json!(anchor + 3600)),
        ("recent", json!(anchor - 15)),
        ("minutes", json!(anchor - 150)),
        ("hours", json!(anchor - 7500)),
        ("days", json!(anchor - 180000)),
        ("negative", json!(-1)),
        ("fractional", json!(-0.5)),
        ("wide-positive", wide(1e100)?),
        ("wide-negative", wide(-1e100)?),
        ("maximum-positive", wide(f64::MAX)?),
        ("maximum-negative", wide(-f64::MAX)?),
        (
            "wide-integer",
            serde_json::from_str("-1208925819614629174706177")?,
        ),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let observed = runtime.block_on(async {
        let mut observed = Vec::new();
        for (index, (name, timestamp)) in cases.into_iter().enumerate() {
            let root = temporary.path().join(format!("native-{index}"));
            fs::create_dir_all(root.join("dashboard"))?;
            let record = json!({"id":"a","name":"Activity α 日本語","notes":"Preserved",
                                "_created":1.0,"_last_active":timestamp});
            let stored = serde_json::to_vec_pretty(&record)?;
            let path = root.join("dashboard/a.json");
            fs::write(&path, &stored)?;
            let app = router(root, "fixture-api", "fixture-host")?;
            let started = clock()?;
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
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let ended = clock()?;
            assert!(ended >= started, "Activity request clock moved backwards");
            assert_eq!(
                fs::read(&path)?,
                stored,
                "Activity read rewrote stored bytes: {name}"
            );
            let body = match serde_json::from_slice::<Value>(&bytes) {
                Ok(body) => body,
                Err(_) => Value::String(String::from_utf8(bytes.to_vec())?),
            };
            observed.push(json!({"name":name,"record":record,"started":started,
                                 "ended":ended,"status":status,"body":body}));
        }
        Ok::<_, Box<dyn std::error::Error>>(observed)
    })?;
    let input = temporary.path().join("observations.json");
    fs::write(&input, serde_json::to_vec(&observed)?)?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,os,sys,tempfile
from pathlib import Path
from unittest.mock import patch
repository,parent,input_file=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from fastapi.testclient import TestClient
from numera.server import create_app
from run_logging import run_log
rows=json.loads(input_file.read_text(encoding='utf-8'))
expected=[]
with run_log('relative-activity-oracle',parent/'logs') as log:
    with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
        with tempfile.TemporaryDirectory(prefix='activity-',dir=parent) as directory:
            root=Path(directory)
            app=create_app(data_dir=directory,start_scheduler=False)
            path=root/'dashboard/a.json'
            path.parent.mkdir(exist_ok=True)
            with TestClient(app,raise_server_exceptions=False) as client:
                for row in rows:
                    stored=json.dumps(row['record'],ensure_ascii=True).encode('utf-8')
                    path.write_bytes(stored)
                    endpoints=[]
                    for now in (row['started'],row['ended']):
                        with patch('numera.convert.time.time',return_value=now):
                            response=client.get('/api/profiles',headers={'authorization':'Bearer fixture-api'})
                        body=response.json() if response.headers.get('content-type','').startswith('application/json') else response.text
                        endpoints.append({'status':response.status_code,'body':body})
                        assert path.read_bytes()==stored,'Original activity read rewrote stored bytes'
                    assert endpoints[0]==endpoints[1],'Unstable activity request clock interval: '+row['name']
                    expected.append(endpoints[0])
    log.info('Observed stable activity HTTP cases=%s',len(expected))
    print(json.dumps(expected,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).arg(input);
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original activity HTTP observer failed");
    let expected: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(expected.len(), 24, "Activity HTTP observations missing");
    assert_eq!(observed.len(), expected.len());
    for (actual, expected) in observed.iter().zip(expected) {
        assert_eq!(
            actual["status"], expected["status"],
            "Relative activity HTTP status differs: {}",
            actual["name"]
        );
        assert_eq!(
            actual["body"], expected["body"],
            "Relative activity HTTP result differs: {}",
            actual["name"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn profile_http_read_repairs_only_view_and_reports_uncertain_claim_as_running()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-read.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Profile read cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("profile-read")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            fs::create_dir(root.join("profiles"))?;
            for (filename, profile) in case["engines"]
                .as_object()
                .ok_or("Engine profiles missing")?
            {
                let bytes = match profile.as_str() {
                    Some(text) => text.as_bytes().to_vec(),
                    None => serde_json::to_vec(profile)?,
                };
                fs::write(root.join("profiles").join(filename), bytes)?;
            }
            let profiles = case["profiles"].as_object().ok_or("Profiles missing")?;
            let mut originals = Vec::new();
            for (filename, profile) in profiles {
                let bytes = match profile.as_str() {
                    Some(text) => text.as_bytes().to_vec(),
                    None => serde_json::to_vec(profile)?,
                };
                fs::write(root.join("dashboard").join(filename), &bytes)?;
                originals.push((filename, bytes));
            }
            if let Some(claim) = case["claim"].as_str() {
                fs::create_dir(root.join("sessions"))?;
                fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
            }
            let app = router(root.clone(), "fixture-api", "fixture-host")?;
            let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get("/api/profiles")
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
            if case["name"] == "last-active" {
                let label = body[0]["lastActive"]
                    .as_str()
                    .ok_or("Last active label missing")?;
                assert!(
                    label.ends_with("d ago"),
                    "Actual old timestamp must use days"
                );
                body[0]["lastActive"] = case["body"][0]["lastActive"].clone();
            }
            if case["name"] == "engine-adopt" {
                let ended = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
                assert!(
                    body[0]["_created"]
                        .as_f64()
                        .is_some_and(|time| time >= started && time <= ended),
                    "HTTP adoption creation must be within the request's clock bounds"
                );
                body[0]["_created"] = case["body"][0]["_created"].clone();
                let adopted: Value =
                    serde_json::from_slice(&fs::read(root.join("dashboard/engine-fixture.json"))?)?;
                let mut expected_view = adopted.clone();
                assert_eq!(
                    adopted["_created"],
                    serde_json::from_slice::<Value>(&bytes)?[0]["_created"]
                );
                expected_view["_created"] =
                    case["adopted"]["engine-fixture.json"]["_created"].clone();
                assert_eq!(expected_view, case["adopted"]["engine-fixture.json"]);
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            for (filename, original) in originals {
                assert_eq!(
                    Value::Bool(original == fs::read(root.join("dashboard").join(filename))?),
                    case["unchanged"][filename]
                );
            }
            if case["name"] == "engine-damaged" {
                assert!(fs::read_dir(root.join("dashboard"))?.next().is_none());
            }
            temporary.cleanup()?;
        }
        Ok(())
    })
}
