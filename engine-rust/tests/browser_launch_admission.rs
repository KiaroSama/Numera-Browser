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
