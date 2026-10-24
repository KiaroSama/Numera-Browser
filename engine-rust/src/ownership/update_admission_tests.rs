use super::update_admission::Reservation;
use crate::{common, ownership::session::Session, storage::locks::DataLock};
use serde_json::{Value, json};
use std::{fs, io, process::Command, sync::mpsc, time::Duration};

fn gate(root: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(root.join(".updates"))?;
    fs::write(root.join(".updates/admission.json"), bytes)
}

#[test]
fn native_session_reserves_before_helper_birth_and_closed_gates_never_expire()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native-update-admission")?;
    let root = temporary.path();
    for bytes in [
        b"malformed".as_slice(),
        b"{}",
        br#"{"schema":1,"state":"quiesced","created_at":0,"owner_pid":999999}"#,
    ] {
        gate(root, bytes)?;
        let error = Session::admit(root, "profile", "fixture-host")
            .err()
            .ok_or("Closed admission accepted a session")?;
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(!root.join("sessions/profile.json").exists());
        fs::remove_file(root.join(".updates/admission.json"))?;
    }
    let mut session = Session::admit(root, "profile", "fixture-host")?;
    let records = fs::read_dir(root.join(".updates/work"))?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        records.len(),
        1,
        "Admission must be durable before any helper exists"
    );
    let record: Value = serde_json::from_slice(&fs::read(records[0].path())?)?;
    assert_eq!(record["schema"], 1);
    assert_eq!(record["pid"], std::process::id());
    assert!(record["token"].as_u64().is_some());
    gate(root, b"{}")?;
    let mut command = Command::new(root.join("must-not-start"));
    assert_eq!(
        session
            .spawn(&mut command, "fixture")
            .err()
            .ok_or("Closed gate spawned")?
            .kind(),
        io::ErrorKind::WouldBlock
    );
    session.finish(Duration::ZERO)?;
    assert_eq!(fs::read_dir(root.join(".updates/work"))?.count(), 0);
    assert_eq!(fs::read(root.join(".updates/admission.json"))?, b"{}");
    drop(session);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn admission_and_gate_decision_share_the_same_root_transaction()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native-update-transaction")?;
    let root = temporary.path().to_owned();
    let lock = DataLock::transaction(&root)?;
    let guard = lock.acquire(Duration::from_secs(5))?;
    let (ready, waiting) = mpsc::sync_channel(1);
    let (result, receive) = mpsc::sync_channel(1);
    let worker_root = root.clone();
    let worker = std::thread::spawn(move || {
        ready.send(()).expect("Fixture receiver disappeared");
        let outcome = Reservation::begin(&worker_root, "fixture")
            .err()
            .map(|error| error.kind());
        result
            .send(outcome)
            .expect("Fixture result receiver disappeared");
    });
    let started = waiting.recv_timeout(Duration::from_secs(5));
    let closed = gate(&root, b"{}");
    drop(guard);
    let observed = receive.recv_timeout(Duration::from_secs(35));
    worker.join().map_err(|_| "Admission fixture panicked")?;
    started?;
    closed?;
    assert_eq!(observed?, Some(io::ErrorKind::WouldBlock));
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn reservation_release_preserves_foreign_evidence_and_does_not_need_open_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native-update-release")?;
    let root = temporary.path();
    let mut reservation = Reservation::begin(root, "fixture")?;
    let records = fs::read_dir(root.join(".updates/work"))?.collect::<Result<Vec<_>, _>>()?;
    let path = records[0].path();
    fs::write(
        &path,
        serde_json::to_vec(&json!({"schema":1,"foreign":true}))?,
    )?;
    assert!(reservation.release().is_err());
    reservation.retain();
    assert!(
        path.is_file(),
        "A replaced receipt must remain for recovery"
    );
    fs::remove_file(&path)?;
    let mut reservation = Reservation::begin(root, "fixture")?;
    gate(root, b"{}")?;
    reservation.release()?;
    assert_eq!(fs::read_dir(root.join(".updates/work"))?.count(), 0);
    assert!(root.join(".updates/admission.json").is_file());
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn closed_gate_refuses_detached_jobs_before_record_or_supervisor_birth()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native-update-jobs")?;
    let root = temporary.path();
    gate(root, b"{}")?;
    let jobs = crate::jobs::JobStore::new(root.into());
    let argv = vec!["list".into()];
    let failure = jobs
        .spawn_detached(crate::job_supervisor::SpawnOptions {
            executable: &std::env::current_exe()?,
            argv: &argv,
            command: "list",
            profile: None,
            password: None,
        })
        .err()
        .ok_or("Closed gate admitted a detached job")?;
    assert_eq!(failure.kind(), io::ErrorKind::WouldBlock);
    assert!(!root.join("jobs").exists());
    temporary.cleanup()?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn linked_update_parent_or_work_directory_never_becomes_an_admission_path()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let temporary = common::TempDirectory::new("native-update-links")?;
    let root = temporary.path();
    let outside = root.join("outside");
    fs::create_dir(&outside)?;
    symlink(&outside, root.join(".updates"))?;
    assert_eq!(
        Reservation::begin(root, "fixture")
            .err()
            .ok_or("Linked parent accepted")?
            .kind(),
        io::ErrorKind::WouldBlock
    );
    fs::remove_file(root.join(".updates"))?;
    fs::create_dir(root.join(".updates"))?;
    symlink(&outside, root.join(".updates/work"))?;
    assert_eq!(
        Reservation::begin(root, "fixture")
            .err()
            .ok_or("Linked work directory accepted")?
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read_dir(&outside)?.count(), 0);
    temporary.cleanup()?;
    Ok(())
}
