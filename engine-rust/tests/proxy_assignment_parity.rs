use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{browser::adapter::Adapter, http::proxy_routes::router};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};
use tower::ServiceExt;
mod common;
#[path = "common/proxy_relay.rs"]
mod proxy_relay;

#[test]
fn proxy_http_archive_and_assignment_preserve_original_stores_and_refusals()
-> Result<(), Box<dyn std::error::Error>> {
    let source_root = common::TempDirectory::new("proxy-assignment-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let mut command = Command::new(python);
    command.args(["-u", "-c", r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from proxy_assignment_cases import cases,address_cases,relay_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'addresses':address_cases(),'relays':relay_cases()}}),flush=True)
"#]).env("PYTHONUTF8", "1").env("FIXTURE_ROOT", source_root.path().join("source"))
        .env("PYTHONPATH", std::env::join_paths([repository.join("scripts/migration"), repository.join("engine")])?);
    let mut source = Adapter::spawn(
        source_root.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-proxy-assignment",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    source_root.cleanup()?;
    let addresses = document["addresses"]
        .as_array()
        .ok_or("Address observations missing")?;
    assert!(
        addresses.len() >= 100,
        "Source prefix-boundary coverage missing"
    );
    for case in addresses {
        let ip = case["ip"]
            .as_str()
            .ok_or("Source address missing")?
            .parse()?;
        assert_eq!(
            numera_core::network::public_ip::is_global(ip),
            case["global"]
                .as_bool()
                .ok_or("Source address verdict missing")?,
            "{}",
            case["ip"]
        );
    }
    let relays = document["relays"]
        .as_array()
        .ok_or("Relay observations missing")?;
    assert_eq!(relays.len(), 4);
    for case in relays {
        proxy_relay::observe(case)?;
    }
    let cases = document["cases"]
        .as_array()
        .ok_or("Proxy assignment cases missing")?;
    assert_eq!(cases.len(), 28);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        for case in cases {
            let temporary = common::TempDirectory::new("proxy-assignment-native")?;
            let root = temporary.path();
            for kind in ["profiles", "dashboard"] {
                fs::create_dir(root.join(kind))?;
                for (name, record) in case["before"][kind]
                    .as_object()
                    .ok_or("Source records missing")?
                {
                    fs::write(root.join(kind).join(name), serde_json::to_vec(record)?)?;
                }
            }
            fs::write(
                root.join("proxies.json"),
                serde_json::to_vec(&case["rows"])?,
            )?;
            let before: Vec<_> = [
                "profiles/a.json",
                "profiles/b.json",
                "dashboard/a.json",
                "dashboard/b.json",
                "proxies.json",
            ]
            .into_iter()
            .map(|path| fs::read(root.join(path)).map(|bytes| (path, bytes)))
            .collect::<Result<_, _>>()?;
            let started = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs_f64();
            let mut builder = Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/proxies/{}",
                    case["verb"].as_str().ok_or("Verb missing")?
                ))
                .header("content-type", "application/json");
            if case["authorized"] == true {
                builder = builder.header("authorization", "Bearer fixture-api");
            }
            let response = tokio::time::timeout(
                Duration::from_secs(15),
                router(root.into(), "fixture-api")
                    .oneshot(builder.body(Body::from(serde_json::to_vec(&case["input"])?))?),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Source status missing")?,
                "{}",
                case["name"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 65536),
            )
            .await??;
            let body: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(body, case["body"], "{}", case["name"]);
            let finished = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs_f64();
            if case["status"].as_u64().is_some_and(|status| status >= 400) {
                for (path, bytes) in &before {
                    assert_eq!(
                        &fs::read(root.join(path))?,
                        bytes,
                        "Refused {} changed {path}",
                        case["name"]
                    );
                }
            }
            let pool: Value = serde_json::from_slice(&fs::read(root.join("proxies.json"))?)?;
            assert_eq!(pool, case["pool_after"], "{}", case["name"]);
            for kind in ["profiles", "dashboard"] {
                let expected = case["after"][kind]
                    .as_object()
                    .ok_or("Source readback missing")?;
                let paths = fs::read_dir(root.join(kind))?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|value| value == "json")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(paths.len(), expected.len());
                for path in paths {
                    let name = path
                        .file_name()
                        .into_string()
                        .map_err(|_| "Nontext record filename")?;
                    let mut record: Value = serde_json::from_slice(&fs::read(path.path())?)?;
                    let original = expected
                        .get(&name)
                        .ok_or("Unexpected native record")?
                        .clone();
                    // Source pins the clock. Native mutations must use real time;
                    // normalize only verified timestamp fields after checking them.
                    for key in ["created_at", "updated_at", "updatedAt"] {
                        if original[key] == 1000.0 && record[key] != original[key] {
                            assert!(record[key].as_f64().is_some_and(|value| value.is_finite()
                                && value >= started
                                && value <= finished));
                            record[key] = original[key].clone();
                        }
                    }
                    assert_eq!(record, original, "{} {kind}/{name}", case["name"]);
                }
            }
            temporary.cleanup()?;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
