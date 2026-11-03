use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};
mod common;

#[test]
fn restore_copy_candidates_match_original_existing_name_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{process::Command, time::Duration};

    let temporary = common::TempDirectory::new("restore-name-ownership")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import base64,json,sys,zipfile
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/profile-restore.json').read_text(encoding='utf-8'))
base=fixture['cases'][0]['result']
name=base['name'];copy=name+' (copy)';second=name+' (copy 2)'
# Each row names raw engine/dashboard owners, including deliberately divergent twins.
variants=[('unclaimed',[]),('foreign-base',[('other',name,name)]),
 ('own-base',[('fixture-restore',name,name),('other',name,name)]),
 ('own-copy',[('fixture-restore',copy,copy),('other',name,copy)]),
 ('own-dashboard-copy',[('fixture-restore','previous',copy),('other',name,copy)]),
 ('own-engine-copy',[('fixture-restore',copy,'previous'),('other',name,copy)]),
 ('foreign-copy',[('other',name,copy)]),
 ('own-second',[('fixture-restore',second,second),('other',name,copy),('third',second,second)]),
 ('foreign-chain',[('other',name,copy),('third',second,second)]),
 ('case-distinct',[('other',name.upper(),copy)]),
 ('unicode-distinct',[('other',name+' 日本語',copy)]),
 ('own-diverged-base',[('fixture-restore','previous',name),('other',name,name)])]
observed=[]
with run_log('restore-name-ownership-oracle',parent/'logs') as log:
 source=parent/'incoming.zip'
 with zipfile.ZipFile(source,'w',zipfile.ZIP_STORED) as archive:
  archive.writestr('profile.json',json.dumps(base))
  archive.writestr('user-data/Default/restore.bin',b'\x00\xffrestored session\r\n')
 archive_bytes=source.read_bytes()
 for label,rows in variants:
  root=parent/str(len(observed));store=ProfileStore(root)
  initial={}
  for identifier,left,right in rows:
   initial['profiles/'+identifier+'.json']={**base,'id':identifier,'name':left,'notes':'retained record'}
   initial['dashboard/'+identifier+'.json']={'id':identifier,'name':right,'folder':'retained folder',
     '_created':123.0,'_rev':7,'futureRestore':{'text':'日本語','values':[True,42]}}
  for relative,record in initial.items():
   path=root/relative;path.parent.mkdir(parents=True,exist_ok=True)
   path.write_text(json.dumps(record,ensure_ascii=True),encoding='utf-8')
  old=root/'user-data/fixture-restore/stale.bin'
  old.parent.mkdir(parents=True,exist_ok=True);old.write_bytes(b'old session must be replaced')
  untouched={relative:(root/relative).read_bytes() for relative in initial if 'fixture-restore.json' not in relative}
  with patch('numera.repo.time.time',return_value=1700000000.0):
   result=store.restore(source).to_dict()
  written={path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
    for kind in ('profiles','dashboard') for path in sorted((root/kind).glob('*.json'))}
  assert all((root/relative).read_bytes()==raw for relative,raw in untouched.items())
  assert not old.exists() and source.read_bytes()==archive_bytes
  assert (root/'user-data/fixture-restore/Default/restore.bin').read_bytes()==b'\x00\xffrestored session\r\n'
  assert not list(root.glob('numera-restore-*')) and not list((root/'user-data').glob('*.replaced-*'))
  observed.append({'label':label,'initial':initial,'result':result,'written':written})
 log.info('Observed public restore name cases=%s',len(observed))
 print(json.dumps({'archive':base64.b64encode(archive_bytes).decode('ascii'),'cases':observed},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = match capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    ) {
        Ok(output) => output,
        Err(mut failure) => {
            if failure.settle(Duration::from_secs(5)).is_err() {
                std::mem::forget(temporary);
            }
            return Err(failure.into());
        }
    };
    assert_eq!(
        original.exit, 0,
        "Original restore ownership observer failed"
    );
    let observed: Value = serde_json::from_slice(&original.stdout)?;
    let archive = STANDARD.decode(observed["archive"].as_str().ok_or("Archive missing")?)?;
    let cases = observed["cases"]
        .as_array()
        .ok_or("Restore observations missing")?;
    assert_eq!(cases.len(), 12, "Restore ownership observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let initial = case["initial"]
            .as_object()
            .ok_or("Initial records missing")?;
        for (relative, record) in initial {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().ok_or("Record parent missing")?)?;
            fs::write(path, serde_json::to_vec(record)?)?;
        }
        let untouched: Vec<_> = initial
            .keys()
            .filter(|relative| !relative.ends_with("/fixture-restore.json"))
            .map(|relative| Ok((relative, fs::read(root.join(relative))?)))
            .collect::<Result<_, std::io::Error>>()?;
        let session = root.join("user-data/fixture-restore");
        fs::create_dir_all(&session)?;
        fs::write(session.join("stale.bin"), b"old session must be replaced")?;
        let source = root.join("incoming.zip");
        fs::write(&source, &archive)?;
        let restored = ProfileRepo::new(root.clone(), None).restore(
            &source,
            false,
            "fixture-host",
            1700000000.0,
        )?;
        assert_eq!(
            serde_json::to_value(restored)?,
            case["result"],
            "Restore candidate ownership differs: {}",
            case["label"]
        );
        let mut written = Map::new();
        for kind in ["profiles", "dashboard"] {
            for entry in fs::read_dir(root.join(kind))? {
                let path = entry?.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                {
                    written.insert(
                        path.strip_prefix(&root)?
                            .to_str()
                            .ok_or("Record path is not UTF8")?
                            .replace('\\', "/"),
                        serde_json::from_slice(&fs::read(path)?)?,
                    );
                }
            }
        }
        assert_eq!(
            Value::Object(written),
            case["written"],
            "Restore records differ: {}",
            case["label"]
        );
        for (relative, bytes) in untouched {
            assert_eq!(
                fs::read(root.join(relative))?,
                bytes,
                "Unrelated restore record changed"
            );
        }
        let mut files = Map::new();
        session_files(&session, &session, &mut files)?;
        assert_eq!(
            Value::Object(files),
            serde_json::json!({
                "Default/restore.bin": STANDARD.encode(b"\x00\xffrestored session\r\n")
            })
        );
        assert_eq!(
            fs::read(source)?,
            archive,
            "Restore changed its input archive"
        );
        for directory in [&root, &root.join("user-data")] {
            assert!(
                !fs::read_dir(directory)?.any(|entry| entry.is_ok_and(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("numera-restore-") || name.contains(".replaced-")
                })),
                "Restore left staging or replaced session residue"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

fn session_files(base: &Path, dir: &Path, found: &mut Map<String, Value>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            session_files(base, &path, found)?;
        } else {
            let name = path
                .strip_prefix(base)
                .map_err(std::io::Error::other)?
                .to_str()
                .ok_or_else(|| std::io::Error::other("Invalid session path"))?
                .replace('\\', "/");
            found.insert(name, Value::String(STANDARD.encode(fs::read(path)?)));
        }
    }
    Ok(())
}

#[test]
fn restore_stages_then_replaces_session_and_commits_both_records()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-restore.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Restore cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("restore-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(value)?)?;
            }
            for (name, value) in case["initial_session"]
                .as_object()
                .ok_or("Initial session missing")?
            {
                let path = root.join("user-data").join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    STANDARD.decode(value.as_str().ok_or("File bytes missing")?)?,
                )?;
            }
            if case["damaged_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-restore.json"), b"{ broken")?;
            }
            let source = root.join("incoming.zip");
            fs::write(
                &source,
                STANDARD.decode(case["archive"].as_str().ok_or("ZIP bytes missing")?)?,
            )?;
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.restore(&source, false, "fixture-host", 1700000000.0) {
                Ok(profile) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(
                        serde_json::to_value(profile)?,
                        case["result"],
                        "{}",
                        case["name"]
                    );
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let dir = root.join(kind);
                if !dir.exists() {
                    continue;
                }
                for entry in fs::read_dir(dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        written.insert(
                            format!(
                                "{kind}/{}",
                                path.file_name()
                                    .ok_or("Filename missing")?
                                    .to_str()
                                    .ok_or("Invalid filename")?
                            ),
                            serde_json::from_slice(&fs::read(path)?)?,
                        );
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"], "{}", case["name"]);
            let mut session = Map::new();
            session_files(
                &root.join("user-data"),
                &root.join("user-data"),
                &mut session,
            )?;
            assert_eq!(Value::Object(session), case["session"], "{}", case["name"]);
            assert!(!fs::read_dir(&root)?.any(|entry| entry.is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("numera-restore-")
            })));
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn failed_metadata_commit_restores_old_session_and_new_id_keeps_original()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-restore.json"))?;
    let case = &oracle["cases"][0];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("restore-rollback-{}", std::process::id()));
    fs::create_dir_all(root.join("profiles"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let source = root.join("incoming.zip");
        fs::write(
            &source,
            STANDARD.decode(case["archive"].as_str().ok_or("ZIP bytes missing")?)?,
        )?;
        let repo = ProfileRepo::new(root.clone(), None);
        let original = repo.restore(&source, false, "fixture-host", 1700000000.0)?;
        let session = root.join("user-data").join(&original.id);
        fs::write(session.join("old.txt"), b"previous session must survive")?;
        let original_bytes = fs::read(root.join("profiles").join(format!("{}.json", original.id)))?;
        // A real directory obstructs the second metadata write after the session swap.
        let dashboard = root.join("dashboard").join(format!("{}.json", original.id));
        fs::remove_file(&dashboard)?;
        fs::create_dir(&dashboard)?;
        assert!(
            repo.restore(&source, false, "fixture-host", 1700000001.0)
                .is_err()
        );
        assert_eq!(
            fs::read(session.join("old.txt"))?,
            b"previous session must survive"
        );
        assert_eq!(
            fs::read(root.join("profiles").join(format!("{}.json", original.id)))?,
            original_bytes
        );
        assert!(dashboard.is_dir());
        fs::remove_dir(&dashboard)?;
        let copy = repo.restore(&source, true, "fixture-host", 1700000002.0)?;
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.id.len(), 12);
        assert!(copy.id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(copy.name, "restore α (copy)");
        assert_eq!(
            fs::read(
                root.join("user-data")
                    .join(&copy.id)
                    .join("Default/Cookies")
            )?,
            b"fixture-new-session"
        );
        assert_eq!(
            fs::read(session.join("old.txt"))?,
            b"previous session must survive"
        );
        assert_eq!(
            fs::read(root.join("profiles").join(format!("{}.json", original.id)))?,
            original_bytes
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result?;
    Ok(())
}
