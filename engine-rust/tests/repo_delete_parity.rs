use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn direct_session_paths_and_deletion_match_original_without_touching_profiles()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("direct-session-storage")?;
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
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import ProfileStore
from run_logging import run_log
rows=[]
with run_log('direct-session-storage-oracle',parent/'logs') as log:
    for identifier in ['fixture-profile','a_b-9','a'*64,'','../outside','a/b','CON','日本語']:
        root=parent/str(len(rows))
        store=ProfileStore(root)
        try:
            path=store.user_data_path(identifier)
            result=str(path.relative_to(root)).replace('\\','/')
            assert store.user_data_path(identifier)==path
            error=None
        except ValueError as exc:result,error=None,str(exc)
        rows.append({'op':'path','id':identifier,'result':result,'error':error,
                     'directories':sorted(p.name for p in (root/'user-data').iterdir())})
    for mode in ['present','absent','empty-claim','broken-claim','foreign-claim']:
        root=parent/str(len(rows))
        store=ProfileStore(root)
        files={'config.json':b'{"future":[true,7]}','profiles/fixture-profile.json':b'{"future":"engine"}',
               'dashboard/fixture-profile.json':b'{"future":"dashboard"}'}
        if mode!='absent':
            path=store.user_data_path('fixture-profile')
            (path/'nested').mkdir()
            (path/'nested/session.bin').write_bytes(b'\x00\xff\n\r\n')
        claim={'empty-claim':b'{}','broken-claim':b'{broken',
               'foreign-claim':b'{"host":"foreign-fixture","session":"fixture","pid":1}'}.get(mode)
        if claim is not None:files['sessions/fixture-profile.json']=claim
        for name,raw in files.items():
            path=root/name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(raw)
        before={str(p.relative_to(root)).replace('\\','/'):list(p.read_bytes())
                for directory in ['profiles','dashboard','sessions'] for p in (root/directory).glob('*')}
        before['config.json']=list((root/'config.json').read_bytes())
        try:
            result=store.delete_session('fixture-profile')
            store.delete_session('fixture-profile')
            error=None
        except RuntimeError as exc:result,error=None,str(exc)
        after={name:list((root/name).read_bytes()) for name in before}
        rows.append({'op':'delete','mode':mode,'files':{name:list(raw) for name,raw in files.items()},
                     'result':result,'error':error,'unchanged':before==after,
                     'session':(root/'user-data/fixture-profile').exists(),
                     'binary':list((root/'user-data/fixture-profile/nested/session.bin').read_bytes())
                              if (root/'user-data/fixture-profile/nested/session.bin').exists() else None,
                     'tombs':sorted(p.name for p in (root/'user-data').glob('*.deleting-*'))})
    for identifier in ['', '../outside', 'CON', '日本語']:
        root=parent/str(len(rows))
        store=ProfileStore(root)
        (root/'sentinel').write_bytes(b'keep')
        try:
            result=store.delete_session(identifier)
            error=None
        except (RuntimeError,ValueError) as exc:result,error=None,str(exc)
        rows.append({'op':'invalid-delete','id':identifier,'result':result,'error':error,
                     'sentinel':list((root/'sentinel').read_bytes()),
                     'directories':sorted(p.name for p in (root/'user-data').iterdir())})
    for mode in ['relative','alias']:
        root=parent/str(len(rows))
        if mode=='relative':root_arg=Path(os.path.relpath(root))
        else:
            (root/'alias').mkdir(parents=True)
            root_arg=root/'alias'/'..'/'actual'
        store=ProfileStore(root_arg)
        path=store.user_data_path('fixture-profile')
        rows.append({'op':'path-root','mode':mode,'absolute':path.is_absolute(),
                     'same':path==root_arg/'user-data'/'fixture-profile','directory':path.is_dir()})
    log.info('Observed direct session storage cases=%s',len(rows))
    print(json.dumps(rows))
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
    assert_eq!(original.exit, 0, "Original direct-session observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(rows.len(), 19);
    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("user-data"))?;
        let repo = ProfileRepo::new(root.clone(), None);
        if row["op"] == "invalid-delete" {
            fs::write(root.join("sentinel"), b"keep")?;
            match repo.delete_session(
                row["id"].as_str().ok_or("Session ID missing")?,
                "fixture-host",
            ) {
                Ok(()) => {
                    assert!(row["error"].is_null());
                    assert!(row["result"].is_null());
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
            }
            assert_eq!(
                serde_json::json!(fs::read(root.join("sentinel"))?),
                row["sentinel"]
            );
            assert!(fs::read_dir(root.join("user-data"))?.next().is_none());
            assert_eq!(row["directories"], serde_json::json!([]));
            continue;
        }
        if row["op"] == "path-root" {
            let argument = if row["mode"] == "relative" {
                root.strip_prefix(std::env::current_dir()?)?.to_path_buf()
            } else {
                fs::create_dir(root.join("alias"))?;
                root.join("alias/../actual")
            };
            let path =
                ProfileRepo::new(argument.clone(), None).user_data_path("fixture-profile")?;
            assert_eq!(serde_json::json!(path.is_absolute()), row["absolute"]);
            assert_eq!(
                serde_json::json!(path == argument.join("user-data/fixture-profile")),
                row["same"]
            );
            assert_eq!(serde_json::json!(path.is_dir()), row["directory"]);
            continue;
        }
        if row["op"] == "path" {
            match repo.user_data_path(row["id"].as_str().ok_or("Session ID missing")?) {
                Ok(path) => {
                    assert!(
                        row["error"].is_null(),
                        "Direct session path unexpectedly succeeded"
                    );
                    assert_eq!(
                        serde_json::json!(
                            path.strip_prefix(&root)?
                                .to_string_lossy()
                                .replace('\\', "/")
                        ),
                        row["result"]
                    );
                    assert!(path.is_dir());
                    assert_eq!(
                        repo.user_data_path(row["id"].as_str().ok_or("Session ID missing")?)?,
                        path
                    );
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
            }
            let mut names = fs::read_dir(root.join("user-data"))?
                .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
                .collect::<Result<Vec<_>, _>>()?;
            names.sort();
            assert_eq!(serde_json::json!(names), row["directories"]);
            continue;
        }
        let mut originals = Vec::new();
        for (name, raw) in row["files"]
            .as_object()
            .ok_or("Session input files missing")?
        {
            let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Session parent missing")?)?;
            fs::write(&path, &bytes)?;
            originals.push((path, bytes));
        }
        if row["mode"] != "absent" {
            let path = repo.user_data_path("fixture-profile")?.join("nested");
            fs::create_dir(&path)?;
            fs::write(path.join("session.bin"), b"\x00\xff\n\r\n")?;
        }
        let result = repo
            .delete_session("fixture-profile", "fixture-host")
            .and_then(|()| repo.delete_session("fixture-profile", "fixture-host"));
        match result {
            Ok(()) => assert!(
                row["error"].is_null(),
                "Direct session deletion unexpectedly succeeded"
            ),
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                row["error"].as_str(),
                "Direct session refusal differs"
            ),
        }
        for (path, bytes) in originals {
            assert_eq!(fs::read(path)?, bytes, "Session deletion changed metadata");
        }
        assert_eq!(row["unchanged"], true);
        let session = root.join("user-data/fixture-profile");
        assert_eq!(serde_json::json!(session.exists()), row["session"]);
        let binary = session.join("nested/session.bin");
        let bytes = if binary.exists() {
            serde_json::json!(fs::read(binary)?)
        } else {
            Value::Null
        };
        assert_eq!(bytes, row["binary"]);
        assert!(
            row["tombs"]
                .as_array()
                .ok_or("Session tombstones missing")?
                .is_empty()
        );
        assert!(!fs::read_dir(root.join("user-data"))?.any(|entry| {
            entry.is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".deleting-"))
        }));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn deletion_removes_both_copies_and_refuses_unreadable_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-delete.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing delete cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("delete-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("user-data/fixture-delete"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("profiles/fixture-delete.json"),
                serde_json::to_vec(&case["profile"])?,
            )?;
            fs::write(
                root.join("user-data/fixture-delete/fixture-state.txt"),
                "isolated browser state α",
            )?;
            if case["corrupt_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-delete.json"), b"{broken")?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.delete(
                case["reference"].as_str().ok_or("Reference missing")?,
                "fixture-host",
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (key, path) in [
                ("engine_present", "profiles/fixture-delete.json"),
                ("dashboard_present", "dashboard/fixture-delete.json"),
                ("session_present", "user-data/fixture-delete"),
                ("claim_retained", "sessions/fixture-delete.json"),
            ] {
                assert_eq!(
                    root.join(path).exists(),
                    case[key].as_bool().ok_or("Missing side effect")?
                );
            }
            assert!(
                !fs::read_dir(root.join("user-data"))?.any(|entry| entry
                    .is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".deleting-"))),
                "Unreclaimed synthetic tombstone"
            );
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
