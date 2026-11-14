use numera_core::ownership::{claims, session::Session};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use numera_core as core;
#[allow(dead_code)]
#[path = "common/ownership_process.rs"]
mod fixture;

fn compatibility_states() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use sha2::Digest;
    let bytes = include_bytes!("fixtures/process-characterization/claim-states.json");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/process-characterization/manifest.json"
    ))?;
    assert_eq!(
        manifest["source_commit"],
        "13b6d9c6973281fe123353eaec05b29d65781a2d"
    );
    assert_eq!(manifest["bytes"].as_u64(), Some(bytes.len() as u64));
    assert_eq!(
        manifest["sha256"],
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    Ok(serde_json::from_slice(bytes)?)
}

fn source_state(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "native_owned_tree_contains_descendants_before_their_first_capability",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NUMERA_CLAIM_QUERY_ROOT", root);
    let output = core::ownership::capture::run(
        &mut child,
        core::ownership::capture::Limits {
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(10),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        output.exit, 0,
        "Native source-compatibility claim query failed"
    );
    let text = String::from_utf8(output.stdout)?;
    let rows = text
        .lines()
        .filter_map(|line| line.strip_prefix("NATIVE CLAIM STATE "))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1, "Claim observation missing");
    let record: serde_json::Value = serde_json::from_str(rows[0])?;
    Ok(record["state"]
        .as_str()
        .ok_or("Native claim verdict missing")?
        .into())
}

#[test]
fn native_owned_tree_contains_descendants_before_their_first_capability()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(root) = std::env::var_os("NUMERA_CLAIM_QUERY_ROOT") {
        let state = match claims::state(
            &claims::read(Path::new(&root), "fixture-profile"),
            "fixture-host",
        ) {
            claims::State::Free => "free",
            claims::State::Busy => "busy",
            claims::State::Unknown => "unknown",
        };
        println!(
            "\nNATIVE CLAIM STATE {}",
            serde_json::json!({"state":state})
        );
        return Ok(());
    }
    let observed = compatibility_states()?;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("claim-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let mut command = fixture::command("tree")?;
    command
        .arg(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut session_owner = Session::admit(&root, "fixture-profile", "fixture-host")?;
    assert!(
        Session::admit(&root, "fixture-profile", "fixture-host").is_err(),
        "Second opener ignored the lease before helper birth"
    );
    let tree = session_owner.spawn(&mut command, "test session")?;
    let output = tree.child_mut().stdout.take().ok_or("No startup output")?;
    let (send, recv) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(output).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(recv.recv_timeout(Duration::from_secs(10))??.trim(), "ready");
        assert!(!tree.empty()?, "A running tree was called empty");
        let claims::Claim::Record(record) = claims::read(&root, "fixture-profile") else {
            return Err("No durable session claim".into());
        };
        let session = record["session"]
            .as_str()
            .ok_or("No claim session identity")?;
        assert_eq!(
            claims::state(&claims::read(&root, "fixture-profile"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            source_state(&root)?,
            observed["live"]
                .as_str()
                .ok_or("Original busy verdict missing")?,
            "Native observer differs from the original live-claim contract"
        );
        assert!(
            !claims::release(&root, "fixture-profile", "another-session", tree)?,
            "Unrelated session released its claim"
        );
        assert!(
            !claims::clear(&root, "fixture-profile", "fixture-host")?,
            "Live tree cleared its claim"
        );
        assert!(
            !claims::release(&root, "fixture-profile", session, tree)?,
            "Live tree released its claim"
        );
        tree.settle(Duration::from_secs(5))?;
        assert!(tree.empty()?, "Descendant survived the native tree cleanup");
        assert_eq!(
            source_state(&root)?,
            observed["settled"]
                .as_str()
                .ok_or("Original settled verdict missing")?,
            "Native observer differs from the original settled-claim contract"
        );
        Ok(())
    })();
    let cleanup = session_owner.finish(Duration::from_secs(5));
    reader.join().map_err(|_| "Output reader panicked")?;
    cleanup?;
    assert!(matches!(
        claims::read(&root, "fixture-profile"),
        claims::Claim::Absent
    ));
    drop(session_owner);
    let reopened = Session::admit(&root, "fixture-profile", "fixture-host")?;
    drop(reopened);
    std::fs::remove_dir_all(root)?;
    result
}
