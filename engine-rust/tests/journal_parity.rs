use numera_core::vault::journal::{JournalState, classify};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn journal_descriptor_boundaries_match_original_without_repairing_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        ownership::capture,
        vault::journal::{descriptors, usable_descriptor},
    };
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("journal-descriptor-boundaries")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.vault import descriptors,classify
from numera.vault_active_descriptor import usable
from run_logging import run_log
salt='AAECAwQFBgcICQoLDA0ODw=='
good={'salt':salt,'verifier':'opaque-verifier'}
records=[]
for new in [None,{},False,0,'',[],True,1,'bad',[1],{'salt':salt},good]:
    records.append({'version':1,'op':'enable','new':new,**good})
for candidate in [salt,salt[:-3]+'x==','QQ==','QR==','QUI=','QUJ=',
                  'QUJD','QUJD=',salt+'=',salt.rstrip('='),'=='+salt,
                  salt[:4]+'='+salt[4:],' '+salt,salt+'é','',None,False,1]:
    for verifier in ['opaque-verifier',' \t','\x1c','\x1f',chr(0x200b),'',None]:
        records.append({'version':1,'op':'enable','new':{'salt':candidate,'verifier':verifier}})
records += [{'version':1,'op':'rotate','new':good,'old':old} for old in
            [None,{},False,1,'bad',good,{'salt':'QR==','verifier':' '}]]
observed=[]
with run_log('journal-descriptor-oracle',root/'logs') as log:
    root.mkdir(parents=True,exist_ok=True)
    path=root/'vault-migration.json'
    for record in records:
        raw=json.dumps(record).encode('utf-8');path.write_bytes(raw)
        new,old=descriptors(record)
        state,stored=classify(root)
        assert path.read_bytes()==raw,'Original classification changed evidence'
        observed.append({'input':record,'new':new,'old':old,'usable':usable(new),
                         'state':state,'record':stored})
    log.info('Observed journal descriptor cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original journal descriptor observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 145, "Journal descriptor observations missing");
    let root = temporary.path().join("native");
    fs::create_dir_all(&root)?;
    let path = root.join("vault-migration.json");
    for (index, case) in cases.iter().enumerate() {
        let bytes = serde_json::to_vec(&case["input"])?;
        fs::write(&path, &bytes)?;
        let (new, old) = descriptors(&case["input"]);
        let (state, record) = classify(&root);
        let label = match state {
            JournalState::Absent => "absent",
            JournalState::Valid => "valid",
            JournalState::Unreadable => "unreadable",
            JournalState::Unsupported => "unsupported",
        };
        assert_eq!(
            serde_json::json!({"new":new,"old":old,"usable":usable_descriptor(&new),
                       "state":label,"record":record}),
            serde_json::json!({"new":case["new"],"old":case["old"],"usable":case["usable"],
                       "state":case["state"],"record":case["record"]}),
            "Journal descriptor boundary differs for case {index}"
        );
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "Journal classification changed evidence"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn journal_classification_retains_uncertainty_and_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/journal.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing journal cases")?;
    assert!(!cases.is_empty(), "Journal oracle must not be empty");
    let parent = std::env::var_os("NUMERA_TEST_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp"));
    let root = parent.join(format!(
        "journal-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("vault-migration.json");
        for case in cases {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if case["input"] == "broken-json" {
                fs::write(&path, b"{broken")?;
            } else if !case["input"].is_null() {
                fs::write(&path, serde_json::to_vec(&case["input"])?)?;
            }
            let before = fs::read(&path).ok();
            let (state, record) = classify(&root);
            let label = match state {
                JournalState::Absent => "absent",
                JournalState::Valid => "valid",
                JournalState::Unreadable => "unreadable",
                JournalState::Unsupported => "unsupported",
            };
            assert_eq!(
                Value::String(label.into()),
                case["expected"],
                "Journal case {}",
                case["name"]
            );
            assert_eq!(
                record.unwrap_or(Value::Null),
                case["record"],
                "Journal record {}",
                case["name"]
            );
            assert_eq!(
                fs::read(&path).ok(),
                before,
                "Classification must not mutate recovery evidence"
            );
        }
        Ok(())
    })();
    fs::remove_dir_all(&root)?;
    result
}
