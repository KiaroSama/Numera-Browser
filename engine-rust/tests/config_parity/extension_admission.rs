use super::{ConfigStore, Value, common, fs};
use numera_core::{ownership::capture, storage::config::ConfigError};
use std::{
    path::Path,
    process::Command,
    time::{Duration, UNIX_EPOCH},
};

#[test]
fn extension_setter_preserves_original_stored_set_admission_and_cache()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("extension-set-admission")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,os,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.config_store import ConfigStore
from run_logging import run_log
valid=parent/'valid-extension';valid.mkdir(parents=True)
(valid/'manifest.json').write_text('{}',encoding='utf-8')
missing=str(parent/'missing-extension')
settings=[[[]],[{}],[['kept']],[{'nested':1}],['kept',[]],
          [None,False,0,1,1.5],[],{}, {'kept':0},'abc',17,False,None]
raws=[json.dumps({'default_extensions':v,'future':{'kept':['日本語',True]}}).encode('utf-8')
      for v in settings]
raws += [b'[]',b'{broken',bytes([255]),None]
jobs=[(raw,wanted,None) for raw in raws for wanted in ([],[str(valid)],[missing])]
# A same-key replacement must use the instance cache for admission, but a fresh
# mutation snapshot for persistence. All three byte strings have equal length.
cached=b'{"default_extensions":["kept"]}'
bad=b'{"default_extensions":[[0.10]]}'
assert len(cached)==len(bad)
invalid=bytes([255])+cached[1:]
for warm,raw in [(cached,bad),(bad,cached),(cached,invalid)]:
    jobs.extend((raw,wanted,warm) for wanted in ([],[str(valid)],[missing]))
observed=[]
with run_log('extension-set-admission-oracle',parent/'logs') as log:
    for raw,wanted,warm in jobs:
        root=parent/'source'/str(len(observed));root.mkdir(parents=True)
        store=ConfigStore();store.root=root;store.config_path=root/'config.json'
        stamp=1800000000*1000000000
        if warm is not None:
            store.config_path.write_bytes(warm);os.utime(store.config_path,ns=(stamp,stamp))
            store.get_config()
        if raw is not None:
            store.config_path.write_bytes(raw)
            if warm is not None:
                os.utime(store.config_path,ns=(stamp,stamp))
                assert store.config_path.stat().st_mtime_ns==stamp
        try:result,error=store.set_default_extensions(wanted),None
        except (TypeError,ValueError,UnicodeDecodeError) as exc:result,error=None,type(exc).__name__
        after=store.config_path.read_bytes() if store.config_path.exists() else None
        observed.append({'raw':list(raw) if raw is not None else None,
                         'warm':list(warm) if warm is not None else None,'wanted':wanted,
                         'result':result,'error':error,'after':list(after) if after is not None else None})
    log.info('Observed extension set admission cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path());
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
        "Original extension set admission observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 60, "Extension set admission cases missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let store = ConfigStore::new(root);
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        let warm: Option<Vec<u8>> = serde_json::from_value(case["warm"].clone())?;
        let stamp = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        if let Some(warm) = &warm {
            fs::write(&path, warm)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_modified(stamp)?;
            store.read()?;
        }
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
            if warm.is_some() {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(stamp)?;
                assert_eq!(fs::metadata(&path)?.modified()?, stamp);
            }
        }
        let wanted: Vec<String> = serde_json::from_value(case["wanted"].clone())?;
        let result = store.set_default_extensions(wanted);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "Stored extension set admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(serde_json::to_value(value)?, case["result"]),
            Err(ConfigError::Io(ref error)) if error.kind() == std::io::ErrorKind::InvalidData => {
                assert_eq!(case["error"], "UnicodeDecodeError");
            }
            Err(error) => assert_eq!(Some(error.source_kind()), case["error"].as_str()),
        }
        let expected: Option<Vec<u8>> = serde_json::from_value(case["after"].clone())?;
        assert_eq!(
            fs::read(&path).ok(),
            expected,
            "Extension setter bytes differ: case={index}"
        );
        if !case["error"].is_null() {
            assert_eq!(
                fs::read(&path).ok(),
                raw,
                "Refused setter replaced settings"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}
