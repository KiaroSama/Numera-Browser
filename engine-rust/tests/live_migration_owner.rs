use numera_core::{
    ownership::{
        identity::{alive, started_at},
        tree::OwnedTree,
    },
    vault::recovery::{needs_recovery, recover},
};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
fn a_live_migration_is_not_recovered_until_its_exact_owner_is_gone()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/recovery.json"))?;
    let mut journal = fixture["cases"][1]["journal"].clone();
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?;
    let mut command = Command::new(python);
    command
        .args([
            "-u",
            "-c",
            "import threading; print('ready',flush=True); threading.Event().wait()",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut tree = OwnedTree::spawn(&mut command)?;
    let output = tree
        .child_mut()
        .stdout
        .take()
        .ok_or("Readiness pipe missing")?;
    let (send, recv) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let value = BufReader::new(output).read_line(&mut line).map(|_| line);
        let _ = send.send(value);
    });
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("live-migration-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(recv.recv_timeout(Duration::from_secs(10))??.trim(), "ready");
        let token = started_at(tree.pid())?.ok_or("Owner identity missing")?;
        journal["pid"] = tree.pid().into();
        journal["token"] = token.into();
        let bytes = serde_json::to_vec(&journal)?;
        fs::write(root.join("vault-migration.json"), &bytes)?;
        fs::write(root.join("config.json"), b"{}")?;
        assert!(alive(tree.pid(), Some(token))?);
        assert!(
            !needs_recovery(&root)?,
            "Live migration mistaken for abandoned work"
        );
        assert!(
            !recover(&root, Some("fixture-recovery-master"), None)?,
            "Live migration was recovered by a second writer"
        );
        assert_eq!(fs::read(root.join("vault-migration.json"))?, bytes);
        tree.settle(Duration::from_secs(5))?;
        assert!(!alive(tree.pid(), Some(token))?);
        assert!(needs_recovery(&root)?);
        assert!(recover(&root, Some("fixture-recovery-master"), None)?);
        assert!(!root.join("vault-migration.json").exists());
        Ok(())
    })();
    let cleanup = tree.settle(Duration::from_secs(5));
    reader.join().map_err(|_| "Readiness reader panicked")?;
    cleanup?;
    fs::remove_dir_all(root)?;
    result
}
