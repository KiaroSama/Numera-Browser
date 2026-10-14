use super::*;
use crate::common;

#[test]
fn attachment_readiness_holds_the_admission_decision_and_settles_failed_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("attachment-decision")?;
    let root = temporary.path();
    let repo = crate::storage::repo::ProfileRepo::new(root.into(), None);
    let fingerprint = crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(
        Profile::from_record(&json!({"id":"a","name":"Fixture","fingerprint":fingerprint}))?,
        1000.0,
    )?;
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let activate = |session, _: Profile| {
        let mut command = Command::new(&python);
        command
            .args([
                "-u",
                "-c",
                r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    result={'closing':True} if request['operation']=='close' else {}
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if request['operation']=='close': break
"#,
            ])
            .env("PYTHONUTF8", "1");
        Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(|error| {
            StartupFailed {
                kind: "failed".into(),
                message: error.to_string(),
            }
        })
    };
    let mut decision_seen = false;
    let outcome = registry.open_attachment("a", "Fixture", activate, || {
        // A stop/reopen needs this exact decision. Inspecting try_lock here
        // proves there is no release gap, without timing-dependent sleeps.
        let decision = registry.decision("a")?;
        assert!(matches!(
            decision.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        decision_seen = true;
        assert!(registry.owns("a")?);
        Err(io::Error::other("Controlled readiness failure"))
    });
    assert!(decision_seen);
    assert_eq!(
        outcome
            .expect_err("Failed readiness reported success")
            .message,
        "Controlled readiness failure"
    );
    assert!(!registry.owns("a")?);
    assert!(registry.attached("a")?.is_none());
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    let mut replacement_seen = false;
    let replacement = registry.open_attachment("a", "Fixture", activate, || {
        replacement_seen = true;
        Ok(json!({"port":17778,"generation":"replacement"}))
    })?;
    assert!(replacement_seen);
    assert_eq!(
        replacement,
        Some(json!({"port":17778,"generation":"replacement"}))
    );
    assert_eq!(registry.attached("a")?, replacement);
    assert!(registry.stop("a", Duration::from_secs(5))?);
    assert!(registry.attached("a")?.is_none());
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(registry.shutdown(Duration::from_secs(5)).is_empty());
    drop(registry);
    temporary.cleanup()?;
    Ok(())
}
