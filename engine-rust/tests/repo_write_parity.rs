use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod common;

#[test]
fn single_edit_preserves_callback_results_and_atomic_store_decisions()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::{ownership::capture, storage::repo::RepoError};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("repo-single-edit")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import base64,copy,json,os,sys,warnings
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from run_logging import run_log
fixtures=repository/'engine-rust/tests/fixtures'
base=json.loads((fixtures/'repo-write.json').read_text(encoding='utf-8'))['cases'][0]['initial']
for record in base.values(): record['tags']=['retained-tag']
encrypted=json.loads((fixtures/'repo.json').read_text(encoding='utf-8'))['cases']
modes=('noop','notes-none','notes-return','replacement','rename','clear-tags',
       'callback-error','replace-secret','clear-secret','remove-proxy')
cases=[(reference,mode,base['profiles'],base['dashboard'],{},None)
       for reference in ('id','name') for mode in modes]
cases.extend((reference,'notes-none',base['profiles'],base['dashboard'],{},None)
             for reference in ('missing-id','missing-name'))
for index,mode in ((10,'notes-none'),(10,'replace-secret'),(10,'clear-secret'),
                   (10,'remove-proxy'),(11,'notes-none'),(12,'notes-none')):
    case=encrypted[index]
    cases.append(('id',mode,case['engine'],case['dashboard'],case['config'],case['password']))
observed=[]
with run_log('repo-single-edit-oracle',parent/'logs') as log, warnings.catch_warnings():
    warnings.simplefilter('ignore',RuntimeWarning)
    for reference,mode,engine,dashboard,config,password in cases:
        root=parent/str(len(observed)); store=ProfileStore(root,password=password)
        repo=ProfileRepo(store); profile_id=engine['id']
        inputs={'profiles/'+profile_id+'.json':json.dumps(engine).encode('utf-8'),
                'dashboard/'+profile_id+'.json':json.dumps(dashboard).encode('utf-8'),
                'config.json':json.dumps(config).encode('utf-8')}
        for name,raw in inputs.items():
            path=root/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(raw)
        target={'id':profile_id,'name':engine['name'],'missing-id':'not-present',
                'missing-name':'no such profile name'}[reference]
        calls=[]
        def apply(profile):
            calls.append(copy.deepcopy(profile.to_dict()))
            if mode=='noop': return None
            if mode=='replacement':
                replacement=copy.deepcopy(profile); replacement.notes='replacement α 😀'
                return replacement
            if mode=='rename': profile.name='renamed α'
            elif mode=='clear-tags': profile.tags=[]
            elif mode=='replace-secret': profile.proxy.password='chosen-fixture-secret'
            elif mode=='clear-secret': profile.proxy.password=''
            elif mode=='remove-proxy': profile.proxy=None
            else: profile.notes='edited 日本語'
            if mode=='callback-error': raise RuntimeError('Callback refused edit')
            return profile if mode=='notes-return' else None
        with patch('numera.repo.time.time',return_value=1700000000.0):
            try:
                saved=repo.edit(target,apply)
                result,error=saved.to_dict() if saved is not None else None,None
            except (RuntimeError,ValueError) as exc: result,error=None,str(exc)
        assert len(calls)==(0 if reference.startswith('missing') else 1),'Original callback count differs'
        after={name:(root/name).read_bytes() for name in inputs}
        if reference.startswith('missing') or mode=='callback-error' or error is not None:
            assert after==inputs,'Original refused edit changed protected bytes'
        observed.append({'reference':target,'mode':mode,'password':password,
                         'files':{name:base64.b64encode(raw).decode('ascii') for name,raw in inputs.items()},
                         'calls':calls,'result':result,'error':error,
                         'unchanged':{name:raw==after[name] for name,raw in inputs.items()},
                         'written':{name:json.loads(raw) for name,raw in after.items()}})
    log.info('Observed single-profile edit cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original single-edit observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 28, "Single-edit observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Single-edit inputs missing")?;
        for (name, raw) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
            let bytes = STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?;
            fs::write(&path, &bytes)?;
            assert_eq!(fs::read(path)?, bytes, "Single-edit input bytes differ");
        }
        let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
        let mut calls = Vec::new();
        let mode = case["mode"].as_str().ok_or("Edit mode missing")?;
        let result = repo.edit(
            case["reference"].as_str().ok_or("Reference missing")?,
            |profile| {
                calls.push(serde_json::to_value(&*profile).map_err(std::io::Error::other)?);
                match mode {
                    "noop" => return Ok(None),
                    "replacement" => {
                        let mut replacement = profile.clone();
                        replacement.notes = "replacement α 😀".into();
                        return Ok(Some(replacement));
                    }
                    "rename" => profile.name = "renamed α".into(),
                    "clear-tags" => profile.tags.clear(),
                    "replace-secret" => profile
                        .proxy
                        .as_mut()
                        .ok_or_else(|| RepoError::Message("Proxy missing".into()))?
                        .set_password(Some("chosen-fixture-secret".into())),
                    "clear-secret" => profile
                        .proxy
                        .as_mut()
                        .ok_or_else(|| RepoError::Message("Proxy missing".into()))?
                        .set_password(Some(String::new())),
                    "remove-proxy" => profile.proxy = None,
                    _ => profile.notes = "edited 日本語".into(),
                }
                if mode == "callback-error" {
                    return Err(RepoError::Message("Callback refused edit".into()));
                }
                Ok((mode == "notes-return").then(|| profile.clone()))
            },
            1700000000.0,
        );
        assert_eq!(
            serde_json::to_value(calls)?,
            case["calls"],
            "Single-edit callback differs: case={index}"
        );
        match result {
            Ok(saved) => {
                assert!(
                    case["error"].is_null(),
                    "Single-edit refusal missing: case={index}"
                );
                assert_eq!(
                    serde_json::to_value(saved)?,
                    case["result"],
                    "Single-edit result differs: case={index}"
                );
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "Single-edit refusal differs: case={index}"
            ),
        }
        for (name, raw) in files {
            let before = STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?;
            let after = fs::read(root.join(name))?;
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][name],
                "Single-edit preservation differs: case={index}"
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][name],
                "Single-edit stored state differs: case={index}"
            );
            if case["unchanged"][name] == true {
                assert_eq!(
                    after, before,
                    "Single-edit replaced preserved bytes: case={index}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn saving_both_copies_keeps_dashboard_fields_and_explicit_secret_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing write cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!(
                "repo-write-{}-{index}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        for kind in ["profiles", "dashboard"] {
            fs::create_dir_all(root.join(kind))?;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut before = Vec::new();
            for kind in ["profiles", "dashboard"] {
                let path = root.join(kind).join("fixture-write.json");
                let bytes = serde_json::to_vec(&case["initial"][kind])?;
                fs::write(&path, &bytes)?;
                before.push((kind, path, bytes));
            }
            if !case["other"].is_null() {
                fs::write(
                    root.join("profiles/fixture-other.json"),
                    serde_json::to_vec(&case["other"])?,
                )?;
            }
            let mut profile = Profile::from_record(&case["profile"])?;
            if case["intent"] != "preserve"
                && let Some(proxy) = profile.proxy.as_mut()
            {
                proxy.set_password(
                    case["profile"]["proxy"]["password"]
                        .as_str()
                        .map(str::to_owned),
                );
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.save(profile, 1700000000.0) {
                Ok(profile) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(profile)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (kind, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][kind]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][kind]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn failed_second_store_write_rolls_back_first_store_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let case = &oracle["cases"][0];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "write-failure-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard/fixture-write.json"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("profiles/fixture-write.json");
        let before = serde_json::to_vec(&case["initial"]["profiles"])?;
        fs::write(&path, &before)?;
        let repo = ProfileRepo::new(root.clone(), None);
        assert!(
            repo.save(Profile::from_record(&case["profile"])?, 1700000000.0)
                .is_err()
        );
        assert_eq!(
            fs::read(path)?,
            before,
            "Failed dashboard write lost original engine bytes"
        );
        assert!(
            root.join("dashboard/fixture-write.json").is_dir(),
            "Caller filesystem obstruction changed"
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
