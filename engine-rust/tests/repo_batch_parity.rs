use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};
mod common;

#[test]
fn complete_batch_refusals_match_original_index_order_and_preserved_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("indexed-batch-refusals")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.models import Profile
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from numera.write_contract import WriteRefused
from run_logging import run_log
base=json.loads((repository/'engine-rust/tests/fixtures/repo-write.json').read_text(encoding='utf-8'))['cases'][0]['initial']['profiles']
name=copy.deepcopy(base);name['name']=77
many=copy.deepcopy(name)
for field in ('screen_width','screen_height','viewport_width','viewport_height','hardware_concurrency'):
    many['fingerprint'][field]=0
for field in ('device_memory','cameras','microphones','speakers','max_touch_points','seed'):
    many['fingerprint'][field]=-1
many['fingerprint']['device_pixel_ratio']=0
variants=[([name],'reject'),([base,name],'reject'),([name,base],'reject'),
          ([base,base,name],'reject'),([many],'reject'),([base,many],'reject'),
          ([name,many],'reject'),([many,name],'reject'),([base,name],'unknown'),([base,name],'skip')]
rows=[]
with run_log('indexed-batch-refusals-oracle',parent/'logs') as log:
    for records,policy in variants:
        records=copy.deepcopy(records)
        for index,record in enumerate(records):record['id']=f'{index+16:012x}'
        before=copy.deepcopy(records)
        root=parent/str(len(rows));store=ProfileStore(root);repo=ProfileRepo(store)
        config=root/'config.json';raw=b'{"future":{"keep":[true,17]}}';config.write_bytes(raw)
        marker=root/'profiles/retained.json';marker.write_bytes(b'\xffretained-invalid-text')
        try:repo.save_many([Profile.from_dict(record) for record in records],config_change={'default_engine':'cloak'},on_name_clash=policy)
        except WriteRefused as exc:error=str(exc)
        else:raise AssertionError('Original invalid batch was accepted')
        assert records==before and config.read_bytes()==raw and marker.read_bytes()==b'\xffretained-invalid-text'
        assert sorted(p.name for p in (root/'profiles').glob('*.json'))==['retained.json']
        assert not list((root/'dashboard').glob('*.json'))
        rows.append({'profiles':records,'policy':policy,'error':error})
    log.info('Observed indexed batch refusals=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8","1");
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 524288,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original indexed batch observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 10, "Indexed batch observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let config = b"{\"future\":{\"keep\":[true,17]}}";
        let marker = b"\xffretained-invalid-text";
        fs::write(root.join("config.json"), config)?;
        fs::write(root.join("profiles/retained.json"), marker)?;
        let input = case["profiles"].clone();
        let profiles = input
            .as_array()
            .ok_or("Pending profiles missing")?
            .iter()
            .map(Profile::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let repo = ProfileRepo::new(root.clone(), None);
        let result = repo.save_many(
            profiles,
            Map::from_iter([("default_engine".into(), "cloak".into())]),
            case["policy"].as_str().ok_or("Policy missing")?,
            1700000000.0,
        );
        assert_eq!(
            result.err().map(|error| error.to_string()),
            case["error"].as_str().map(str::to_owned),
            "Complete indexed batch refusal differs: case={index}"
        );
        assert_eq!(input, case["profiles"], "Batch input changed");
        assert_eq!(fs::read(root.join("config.json"))?, config);
        assert_eq!(fs::read(root.join("profiles/retained.json"))?, marker);
        for record in input.as_array().ok_or("Pending profiles missing")? {
            let id = record["id"].as_str().ok_or("Pending id missing")?;
            assert!(!root.join("profiles").join(format!("{id}.json")).exists());
            assert!(!root.join("dashboard").join(format!("{id}.json")).exists());
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn batch_names_and_config_commit_with_both_stores_or_not_at_all()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-batch.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing batch cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("batch-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let before = serde_json::to_vec(&case["config"])?;
            fs::write(root.join("config.json"), &before)?;
            let pending = case["profiles"]
                .as_array()
                .ok_or("Profiles missing")?
                .iter()
                .map(Profile::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            for (name, data) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(data)?)?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            let changes = case["changes"]
                .as_object()
                .ok_or("Changes missing")?
                .clone();
            let policy = case["policy"].as_str().ok_or("Policy missing")?;
            let saved = if policy == "patch" {
                let mut refs = pending.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
                refs.push("missing".into());
                repo.patch_many(
                    &refs,
                    |profile| {
                        profile.tags.push("applied".into());
                        Ok(true)
                    },
                    changes,
                    "reject",
                    1700000000.0,
                )
            } else {
                repo.save_many(
                    pending,
                    changes,
                    if policy.ends_with("-held") || policy == "legacy-list-name" {
                        "reject"
                    } else {
                        policy
                    },
                    1700000000.0,
                )
            };
            match saved {
                Ok(saved) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(saved)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|v| v == "json") {
                        let name = format!(
                            "{kind}/{}",
                            path.file_name()
                                .ok_or("Filename missing")?
                                .to_str()
                                .ok_or("Filename not Unicode")?
                        );
                        written.insert(name, serde_json::from_slice(&fs::read(path)?)?);
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"]);
            let after = fs::read(root.join("config.json"))?;
            assert_eq!(
                before == after,
                case["config_unchanged"]
                    .as_bool()
                    .ok_or("Byte verdict missing")?
            );
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["persisted"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
