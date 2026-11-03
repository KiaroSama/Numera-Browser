use numera_core::{ownership::capture, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};

const TEST: &str =
    "read_warnings::read_repair_warnings_match_original_without_changing_read_results";

#[test]
fn read_repair_warnings_match_original_without_changing_read_results()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(document) = std::env::var_os("NUMERA_READ_WARNING_FIXTURE") {
        let document = std::path::PathBuf::from(document);
        let parent = document.parent().ok_or("Warning fixture parent missing")?;
        let cases: Vec<Value> = serde_json::from_slice(&fs::read(&document)?)?;
        let mut rows = Vec::new();
        for (index, case) in cases.iter().enumerate() {
            let root = parent.join(format!("native-{index}"));
            for (name, value) in case["files"].as_object().ok_or("Warning inputs missing")? {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                fs::write(path, serde_json::to_vec(value)?)?;
            }
            let paths = [
                root.join("profiles/fixture00001.json"),
                root.join("dashboard/fixture00001.json"),
                root.join("config.json"),
            ];
            let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
            let repo = ProfileRepo::new(root, None);
            eprintln!("READ_REPAIR_BEGIN {index}");
            let result = match case["operation"].as_str() {
                Some("resolve") => repo
                    .resolve("fixture00001", 1700000000.0)
                    .map(serde_json::to_value),
                Some("find") => repo
                    .find_by_name("fixture", 1700000000.0)
                    .map(serde_json::to_value),
                Some("list") => repo.list(1700000000.0).map(serde_json::to_value),
                Some("coherent") => repo
                    .coherent("fixture00001", 1700000000.0)
                    .map(serde_json::to_value),
                Some("get") => repo.get("fixture00001").map(serde_json::to_value),
                _ => return Err("Unknown warning operation".into()),
            };
            eprintln!("READ_REPAIR_END {index}");
            let (result, error) = match result {
                Ok(value) => (value?, Value::Null),
                Err(error) => (Value::Null, Value::String(error.to_string())),
            };
            for (path, bytes) in paths.iter().zip(before) {
                assert_eq!(
                    fs::read(path)?,
                    bytes,
                    "Read warning changed stored bytes: {index}"
                );
            }
            rows.push(serde_json::json!({"result": result, "error": error}));
        }
        println!("\nREAD_REPAIR_ROWS={}", serde_json::to_string(&rows)?);
        return Ok(());
    }

    let temporary = crate::common::TempDirectory::new("read-repair-warnings")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys,warnings
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.models import Profile
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases']
rows=[]
with run_log('read-repair-warnings-oracle',parent/'logs') as log:
    for operation in ('resolve','find','list','coherent','get'):
        for mode in ('healthy','proxy-omitted','password-conflict','invalid-dashboard'):
            source=fixture[2 if mode=='proxy-omitted' else 3 if mode=='password-conflict' else 0]
            files={'profiles/fixture00001.json':copy.deepcopy(source['engine']),
                   'dashboard/fixture00001.json':copy.deepcopy(source['dashboard']),'config.json':{}}
            if mode=='invalid-dashboard':files['dashboard/fixture00001.json']['proxy']['type']='UNSUPPORTED'
            root=parent/str(len(rows));store=ProfileStore(root)
            for name,value in files.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text(json.dumps(value),encoding='utf-8')
            before={name:(root/name).read_bytes() for name in files}
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter('always',RuntimeWarning)
                try:
                    if operation=='resolve':result=store.resolve('fixture00001')
                    elif operation=='find':result=store.find_by_name('fixture')
                    elif operation=='list':result=store.list()
                    elif operation=='coherent':result=ProfileRepo(store).coherent('fixture00001')
                    else:result=store.get('fixture00001')
                    result=([p.to_dict() for p in result] if isinstance(result,list)
                            else result.to_dict() if isinstance(result,Profile) else result)
                    error=None
                except (RuntimeError,KeyError,TypeError,ValueError) as exc:result,error=None,str(exc)
            assert all((root/name).read_bytes()==raw for name,raw in before.items()),'Original warning changed inputs'
            observed=[{'category':warning.category.__name__,'message':str(warning.message)} for warning in caught]
            expected=operation in ('resolve','find','list') and mode in ('proxy-omitted','password-conflict')
            assert len(observed)==int(expected),'Original read warning seam was not exercised'
            rows.append({'operation':operation,'mode':mode,'files':files,
                         'result':result,'error':error,'warnings':observed})
    log.info('Observed read repair warning cases=%s',len(rows))
print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path().join("original"));
    let limits = || capture::Limits {
        wall: Duration::from_secs(30),
        idle: Duration::from_secs(20),
        bytes: 1_048_576,
        cleanup: Duration::from_secs(5),
    };
    let original = capture::run(&mut command, limits())?;
    assert_eq!(original.exit, 0, "Original read-warning observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 20, "Read-warning observations missing");
    let document = temporary.path().join("observations.json");
    fs::write(&document, &original.stdout)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("NUMERA_READ_WARNING_FIXTURE", &document);
    let native = capture::run(&mut command, limits())?;
    assert_eq!(native.exit, 0, "Native read-warning observer failed");
    let stdout = String::from_utf8(native.stdout)?;
    assert!(
        stdout.contains("running 1 test") && stdout.contains("test result: ok. 1 passed; 0 failed"),
        "Native warning test did not run"
    );
    let documents = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("READ_REPAIR_ROWS="))
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 1, "Native warning observations missing");
    let actual: Vec<Value> = serde_json::from_str(documents[0])?;
    assert_eq!(actual.len(), cases.len());
    let stderr = String::from_utf8(native.stderr)?;
    let mut lines = stderr.lines();
    for (index, (case, actual)) in cases.iter().zip(actual).enumerate() {
        assert_eq!(
            lines.next(),
            Some(format!("READ_REPAIR_BEGIN {index}").as_str())
        );
        let end = format!("READ_REPAIR_END {index}");
        let mut warnings = Vec::new();
        loop {
            let line = lines.next().ok_or("Native warning boundary missing")?;
            if line == end {
                break;
            }
            let (category, message) = line.split_once(": ").ok_or("Unexpected read diagnostic")?;
            warnings.push(serde_json::json!({"category":category,"message":message}));
        }
        assert_eq!(
            Value::Array(warnings),
            case["warnings"],
            "Read repair warning differs: case={index}"
        );
        assert_eq!(
            actual["error"], case["error"],
            "Read repair refusal differs: case={index}"
        );
        assert_eq!(
            actual["result"], case["result"],
            "Read repair result differs: case={index}"
        );
    }
    assert!(lines.next().is_none(), "Unattributed native warning output");
    temporary.cleanup()?;
    Ok(())
}
