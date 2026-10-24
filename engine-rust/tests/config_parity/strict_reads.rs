use super::{ConfigStore, Value, common, fs};
use numera_core::{ownership::capture, storage::config::ConfigError};
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};

#[test]
fn settings_getters_match_original_strict_text_admission_and_values()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-getters")?;
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
from numera.config_store import ConfigStore
from numera import drivers
from run_logging import run_log
drivers.is_installed=lambda name:name=='camoufox'
settings=[bytes([255]),bytes([0xc3,0x28]),bytes([0xe2,0x82]),b'{}',b'{broken',b'[]',None]
settings += [json.dumps(value).encode('utf-8') for value in [
    {'default_engine':'missing-sdk','headless_launch':True},
    {'default_engine':False,'headless_launch':[]},
    {'default_engine':['custom'],'headless_launch':'yes'},
    {'default_engine':7,'headless_launch':0}]]
observed=[]
with run_log('strict-settings-getters-oracle',parent/'logs') as log:
    for operation in ['read','engine','headless']:
        for raw in settings:
            root=parent/str(len(observed));root.mkdir(parents=True)
            store=ConfigStore();store.root=root;store.config_path=root/'config.json'
            if raw is not None:store.config_path.write_bytes(raw)
            try:
                result=(store.get_config() if operation=='read' else
                        store.get_default_engine() if operation=='engine' else store.get_headless_launch())
                error=None
            except UnicodeDecodeError:result,error=None,'UnicodeDecodeError'
            assert (store.config_path.read_bytes() if store.config_path.exists() else None)==raw
            observed.append({'operation':operation,'raw':list(raw) if raw is not None else None,
                             'result':result,'error':error})
    log.info('Observed strict settings getter cases=%s',len(observed))
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
        "Original strict settings getter observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 33);
    let installed = BTreeMap::from([("camoufox".into(), true)]);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
        }
        let store = ConfigStore::new(root);
        let result = match case["operation"].as_str().ok_or("Operation missing")? {
            "read" => store.read().map(Value::Object),
            "engine" => store.default_engine(&installed),
            "headless" => store.headless_launch().map(Value::Bool),
            _ => return Err("Unknown getter operation".into()),
        };
        assert_eq!(
            result.is_err(),
            case["error"] == "UnicodeDecodeError",
            "Strict settings getter admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(value, case["result"], "Getter value differs: case={index}"),
            Err(error) => assert!(matches!(error, ConfigError::Io(ref e)
                if e.kind() == std::io::ErrorKind::InvalidData)),
        }
        assert_eq!(fs::read(path).ok(), raw, "Getter changed settings bytes");
    }
    temporary.cleanup()?;
    Ok(())
}
