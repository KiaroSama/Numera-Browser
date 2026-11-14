use numera_core::ownership::identity::{alive, started_at};

#[test]
fn native_birth_token_matches_original_runtime_for_the_same_live_process()
-> Result<(), Box<dyn std::error::Error>> {
    let pid = std::process::id();
    let token = started_at(pid)?.ok_or("Current process has no native birth identity")?;
    assert!(alive(pid, Some(token))?);
    assert!(
        !alive(pid, Some(token.saturating_add(1)))?,
        "Reused PID identity accepted"
    );
    assert!(!alive(0, None)?);
    let mut child = ownership_process::command("identity")?;
    child.arg(pid.to_string());
    let output = numera_core::ownership::capture::run(
        &mut child,
        numera_core::ownership::capture::Limits {
            wall: std::time::Duration::from_secs(10),
            idle: std::time::Duration::from_secs(10),
            bytes: 4096,
            cleanup: std::time::Duration::from_secs(3),
        },
    )?;
    assert_eq!(output.exit, 0, "Independent kernel birth query failed");
    let source: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    #[cfg(target_os = "macos")]
    let source = {
        use sha2::Digest;
        let line = source["lstart"]
            .as_str()
            .ok_or("process birth text absent")?;
        let digest = sha2::Sha256::digest(line.as_bytes());
        digest[..6]
            .iter()
            .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
    };
    #[cfg(not(target_os = "macos"))]
    let source = source["token"]
        .as_u64()
        .ok_or("process birth token absent")?;
    assert_eq!(
        token, source,
        "A reused PID must be distinguished by the same birth token"
    );
    Ok(())
}
use numera_core as core;
#[path = "common/ownership_process.rs"]
mod ownership_process;
