use numera_core::vault::completion::problems;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn completion_diagnostics_match_original_distinct_paths_and_readable_holders()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("completion-diagnostics")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.vault_recovery import completion_problems
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/completion.json').read_text(encoding='utf-8'))['cases']
descriptor=fixture[1]['descriptor'];good=fixture[4]['files']['proxies.json'][0]['pass']
bad='enc:v9:fixture'
inputs=[
 {'profiles/a.json':{'proxy':{'password':bad}},'dashboard/a.json':{'proxy':{'pass':bad}}},
 {'proxies.json':[42,{'pass':bad}]},
 {'proxies.json':[{'pass':bad},42]},
 {'proxies.json':{'pass':bad}},
 {'proxies.json':[42,{'pass':good}]},
 {'proxies.json':[{'pass':42},{'pass':bad}]},
 {'profiles/a.json':[], 'dashboard/a.json':{'proxy':{'pass':bad}}},
 {'proxies.json':[{'pass':'plain'},{'pass':bad}]},
 {'proxies.json':[42,{'pass':'plain'}]},
 {'profiles/a.json':{'proxy':{'password':bad}},'dashboard/a.json':{'proxy':{'pass':bad}},'proxies.json':[{'pass':bad}]},
 {'profiles/a.json':{'proxy':None},'proxies.json':[]},
 {'proxies.json':[{'pass':good}]},
]
observed=[]
with run_log('completion-diagnostics-oracle',parent/'logs') as log:
    for index,files in enumerate(inputs):
        root=parent/str(index);root.mkdir(parents=True)
        before={}
        for name,data in files.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
            raw=json.dumps(data).encode('utf-8');path.write_bytes(raw);before[name]=raw
        for password in ['fixture-completion-master',None,'wrong-fixture-master']:
            expected=[p._asdict() for p in completion_problems(root,descriptor,password)]
            assert all((root/name).read_bytes()==raw for name,raw in before.items())
            observed.append({'files':files,'descriptor':descriptor,'password':password,'expected':expected})
    log.info('Observed completion diagnostic cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original completion diagnostic observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 36);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(&root)?;
        let mut before = Vec::new();
        for (name, data) in case["files"].as_object().ok_or("Files missing")? {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            let raw = serde_json::to_vec(data)?;
            fs::write(&path, &raw)?;
            before.push((path, raw));
        }
        assert_eq!(
            serde_json::to_value(problems(
                &root,
                &case["descriptor"],
                case["password"].as_str()
            )?)?,
            case["expected"],
            "Completion diagnostic differs for case {index}"
        );
        for (path, raw) in before {
            assert_eq!(fs::read(path)?, raw, "Completion changed evidence");
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn only_authenticated_readable_generation_can_complete_vault()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/completion.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing completion cases")?;
    assert!(!cases.is_empty());
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp");
    for (index, case) in cases.iter().enumerate() {
        let root = parent.join(format!(
            "completion-{}-{index}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let files = case["files"].as_object().ok_or("Missing files")?;
            let mut before = Vec::new();
            for (name, value) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                before.push((path, bytes));
            }
            let actual = problems(&root, &case["descriptor"], case["password"].as_str())?;
            assert_eq!(serde_json::to_value(actual)?, case["expected"]);
            for (path, bytes) in before {
                assert_eq!(fs::read(path)?, bytes, "Completion changed evidence");
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
