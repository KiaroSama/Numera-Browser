use numera_core::{
    browser::launch::{self, PreparedLaunch},
    models::Profile,
    ownership::{claims, session::Session},
};
use serde_json::{Map, Value, json};
use std::process::Command;
mod common;

#[test]
fn admitted_launch_refuses_invalid_backend_and_unprepared_authenticated_proxy_before_sdk_birth()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    for authenticated in [false, true] {
        let temporary = common::TempDirectory::new("launch-admission-refusal")?;
        let root = temporary.path();
        let profile = Profile::from_record(&json!({
            "id":"a", "name":"fixture", "fingerprint":fixture["cases"][0]["input"],
            "proxy":if authenticated { json!({"server":"http://127.0.0.1:9", "username":"fixture", "password":null, "country":null}) } else { Value::Null },
        }))?;
        let session = Session::admit(root, "a", "fixture-host")?;
        let mut command = Command::new("unavailable-sdk-fixture-executable");
        let failure = launch::open(
            session,
            profile,
            &mut command,
            PreparedLaunch {
                backend: if authenticated {
                    "playwright"
                } else {
                    "unknown"
                },
                headless: true,
                extra_args: &[],
                extensions: &[],
                country: None,
                relay: None,
                binary_options: Map::new(),
            },
            1000.0,
        )
        .err()
        .ok_or("Invalid launch was accepted")?;
        assert_eq!(failure.kind, "failed");
        assert!(failure.message.contains(if authenticated {
            "requires a native relay"
        } else {
            "Unknown browser backend"
        }));
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let lease = Session::admit(root, "a", "fixture-host")?;
        drop(lease);
        temporary.cleanup()?;
    }
    Ok(())
}

#[test]
fn camoufox_retains_source_locale_with_marked_country_observation()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let fixture: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let temporary = common::TempDirectory::new("camoufox-launch-policy")?;
    let root = temporary.path();
    let profile = Profile::from_record(&json!({
        "id":"a", "name":"fixture", "fingerprint":fixture["cases"][0]["input"],
        "ip_based":["languages"], "proxy":{"server":"http://127.0.0.1:9", "username":null, "password":null, "country":null},
    }))?;
    let capture = root.join("open.json");
    let script = "import json,os,pathlib,sys; hello=json.loads(sys.stdin.readline()); print(json.dumps({'protocol':1,'id':hello['id'],'ok':True,'result':{'backends':{'camoufox':{'installed':True}}}}),flush=True); opened=json.loads(sys.stdin.readline()); pathlib.Path(os.environ['FIXTURE_CAPTURE']).write_text(json.dumps(opened),encoding='utf-8'); print(json.dumps({'protocol':1,'id':opened['id'],'ok':True,'result':{'opened':True,'backend':'camoufox'}}),flush=True); sys.stdin.readline()";
    let mut command = Command::new(python);
    command
        .args(["-u", "-c", script])
        .env("FIXTURE_CAPTURE", &capture);
    let session = Session::admit(root, "a", "fixture-host")?;
    let mut adapter = launch::open(
        session,
        profile,
        &mut command,
        PreparedLaunch {
            backend: "camoufox",
            headless: true,
            extra_args: &[],
            extensions: &[],
            country: Some("DE"),
            relay: None,
            binary_options: Map::new(),
        },
        1000.0,
    )?;
    let frame: Value = serde_json::from_slice(&std::fs::read(capture)?)?;
    assert_eq!(frame["options"]["locale"], "en-US");
    assert_eq!(frame["options"]["geoip"], true);
    assert_eq!(frame["init_scripts"], json!([]));
    assert!(frame["cdp_policy"].is_null());
    adapter.finish(std::time::Duration::from_secs(5))?;
    drop(adapter);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    temporary.cleanup()?;
    Ok(())
}
