use numera_core::storage::extensions;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn extension_path_whitespace_matches_original_validation() -> Result<(), Box<dyn std::error::Error>>
{
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("extension-text")?;
    let inputs = temporary.path().join("extension inputs");
    let valid = inputs.join("valid");
    fs::create_dir_all(&valid)?;
    fs::create_dir_all(inputs.join("missing-manifest"))?;
    let protected = [
        (
            valid.join("manifest.json"),
            b"{\"name\":\"fixture\"}".to_vec(),
        ),
        (valid.join("fixture.bin"), b"\x00fixture\xff".to_vec()),
    ];
    for (path, bytes) in &protected {
        fs::write(path, bytes)?;
    }
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
from numera.store import unloadable_extensions
from run_logging import run_log

with run_log('extension-text-oracle',root/'logs') as log:
    protected=[root/'valid/manifest.json',root/'valid/fixture.bin']
    before=[path.read_bytes() for path in protected]
    rows=[]
    for marker in map(chr,[0x1c,0x1d,0x1e,0x1f,0x85,0xa0,0x2003,0x202f,
                           0x3000,0x20,0x58,0x394]):
        wrap=lambda text: marker+str(text)+marker
        for entries in [[wrap(root/'valid')],[wrap(root/'missing-manifest')],
                        [wrap(root/'absent')],[wrap('https://example.invalid/extension')],
                        [marker],[wrap(root/'valid'),'',wrap('https://example.invalid/extension')]]:
            rows.append({'entries':entries,'result':unloadable_extensions(entries),
                         'unchanged':[path.read_bytes()==raw for path,raw in zip(protected,before)]})
    log.info('Observed extension text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(&inputs)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original extension text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 72, "Extension text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let entries = case["entries"]
            .as_array()
            .ok_or("Extension entries missing")?
            .iter()
            .map(|value| value.as_str().map(str::to_owned).ok_or("Entry is not text"))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            serde_json::to_value(extensions::unloadable(&entries))?,
            case["result"],
            "Extension path boundary whitespace differs: case={index}"
        );
        assert_eq!(case["unchanged"], serde_json::json!([true, true]));
        for (path, before) in &protected {
            assert_eq!(
                fs::read(path)?,
                *before,
                "Extension validation changed fixture bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn malformed_extension_text_matches_original_refusal_without_changing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("extension-utf8")?;
    let inputs: PathBuf = temporary.path().components().collect();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import webstore
from run_logging import run_log

with run_log('extension-utf8-oracle',root/'logs') as log:
    rows=[]
    for location in ['manifest.json','_locales/en/messages.json']:
        valid=({'name':'日本語','icons':{'16':'icon.png'}} if location=='manifest.json'
               else {'title':{'message':'日本語'}})
        for payload in [b'\xff',b'{"future":"\xc3("}',b'{"future":"\xe2\x82',
                        b'{broken',None,json.dumps(valid,ensure_ascii=False).encode('utf-8')]:
            for operation in ['describe','installed']:
                case=root/'cases'/str(len(rows))
                folder=case/'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
                files={'manifest.json':json.dumps({'name':'__MSG_title__',
                           'default_locale':'en','icons':{'16':'icon.png'}}).encode('utf-8'),
                       '_locales/en/messages.json':b'{"title":{"message":"Fixture"}}',
                       'icon.png':b'\x00fixture\xff'}
                files[location]=payload
                folder.mkdir(parents=True)
                for name,raw in files.items():
                    if raw is not None:
                        path=folder/name
                        path.parent.mkdir(parents=True,exist_ok=True)
                        path.write_bytes(raw)
                try:
                    result=(webstore.describe(folder) if operation=='describe'
                            else webstore.installed(case))
                    error=None
                except UnicodeDecodeError:
                    result,error=None,'UnicodeDecodeError'
                rows.append({'operation':operation,'result':result,'error':error,
                    'files':{name:raw.hex() if raw is not None else None for name,raw in files.items()},
                    'unchanged':[(folder/name).read_bytes()==raw if raw is not None
                                 else not (folder/name).exists() for name,raw in files.items()]})
    log.info('Observed extension resource text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(&inputs)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original extension resource observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 24, "Extension resource observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join("cases").join(index.to_string());
        let folder = root
            .join("extensions")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let result = match case["operation"].as_str() {
            Some("describe") => extensions::describe(&folder)
                .and_then(|value| serde_json::to_value(value).map_err(std::io::Error::other)),
            Some("installed") => extensions::installed(&root)
                .and_then(|value| serde_json::to_value(value).map_err(std::io::Error::other)),
            _ => return Err("Unknown extension observation".into()),
        };
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "Extension resource text admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(value, case["result"], "Extension resource result differs"),
            Err(error) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
        }
        assert_eq!(case["unchanged"], serde_json::json!([true, true, true]));
        for (name, raw) in case["files"].as_object().ok_or("Resource files missing")? {
            let path = folder.join(name);
            if let Some(raw) = raw.as_str() {
                let before = (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()?;
                assert_eq!(fs::read(path)?, before, "Extension resource bytes changed");
            } else {
                assert!(
                    !path.try_exists()?,
                    "Extension resource read created a file"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn extension_icon_integer_ranking_matches_original_selection()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("extension-ranking")?;
    let inputs: PathBuf = temporary.path().components().collect();
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
from numera import webstore
from run_logging import run_log

with run_log('extension-ranking-oracle',root/'logs') as log:
    keys=['1'+'0'*40,'-'+'1'+'0'*40,'128','+128','000128','-0','+0',
          '1_000','1__000','_128','128_','+_128','128.0','1e3','0x80',
          '', 'invalid','-2','-128',chr(0x661)+chr(0x662)+chr(0x668),
          ''.join(map(chr,[0xff11,0xff12,0xff18])),
          '  256\t',chr(0x2003)+'256'+chr(0x3000),
          chr(0x1c)+'256'+chr(0x1f),'0'*4297+'128','0'*4298+'128',
          '9'*4300,'9'*4301,'1_'*4299+'1','1_'*4300+'1']
    variants=[]
    for index,key in enumerate(keys):
        for reverse in [False,True]:
            pairs=[('128','first.png'),(key,'second.png')]
            if reverse:
                pairs.reverse()
            variants.append((f'key-{index}-{int(reverse)}',
                             {'icons':dict(pairs)}))
    variants.extend([
        ('negative-order',{'icons':{'-2':'first.png','-1':'second.png'}}),
        ('zero-tie',{'icons':{'-0':'first.png','invalid':'second.png'}}),
        ('skip-missing',{'icons':{'9'*40:'absent.png','128':'first.png'}}),
        ('skip-outside',{'icons':{'9'*40:'../outside.png','128':'first.png'}}),
        ('skip-nontext',{'icons':{'9'*40:True,'128':'first.png'}}),
        ('action-size',{'icons':{'128':'first.png'},
                        'action':{'default_icon':{'1_000':'second.png'}}}),
        ('icons-win',{'icons':{'1_000':'first.png'},
                      'action':{'default_icon':{'1_000':'second.png'}}}),
        ('action-fallback',{'icons':{'-1':'first.png'},
                            'action':{'default_icon':'second.png'}}),
    ])
    rows=[]
    for label,variant in variants:
        for operation in ['describe','installed']:
            case=root/'cases'/str(len(rows))
            folder=case/'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
            files={'manifest.json':json.dumps({'name':'Fixture',**variant},ensure_ascii=False),
                   'first.png':'first','second.png':'second'}
            folder.mkdir(parents=True)
            for name,text in files.items():
                (folder/name).write_bytes(text.encode('utf-8'))
            result=(webstore.describe(folder) if operation=='describe'
                    else webstore.installed(case))
            rows.append({'label':label,'operation':operation,'result':result,'files':files,
                         'unchanged':[(folder/name).read_bytes()==text.encode('utf-8')
                                      for name,text in files.items()]})
    log.info('Observed extension icon ranking cases=%s',len(rows))
    print(json.dumps({'digit_limit':sys.get_int_max_str_digits(),'cases':rows},ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(&inputs)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original icon ranking observer failed");
    let observed: Value = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(
        observed["digit_limit"], 4300,
        "Original integer limit differs"
    );
    let cases = observed["cases"].as_array().ok_or("Icon cases missing")?;
    assert_eq!(cases.len(), 136, "Icon ranking observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = inputs.join("cases").join(index.to_string());
        let folder = root
            .join("extensions")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let result = match case["operation"].as_str() {
            Some("describe") => serde_json::to_value(extensions::describe(&folder)?)?,
            Some("installed") => serde_json::to_value(extensions::installed(&root)?)?,
            _ => return Err("Unknown icon observation".into()),
        };
        assert_eq!(
            result, case["result"],
            "Extension icon integer ranking differs: case={index} label={}",
            case["label"]
        );
        assert_eq!(case["unchanged"], serde_json::json!([true, true, true]));
        for (name, text) in case["files"].as_object().ok_or("Icon files missing")? {
            assert_eq!(
                fs::read(folder.join(name))?,
                text.as_str().ok_or("Icon fixture is not text")?.as_bytes(),
                "Icon selection changed fixture bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn installed_extensions_preserve_localized_name_and_contained_icon_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-resources.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Extension cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("extension-resource-{}-{index}", std::process::id()));
        let folder = root.join("extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&folder)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, value) in case["files"].as_object().ok_or("Resource files missing")? {
                let path = folder.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    value.as_str().ok_or("Text resource missing")?.as_bytes(),
                )?;
            }
            fs::write(root.join("extensions/outside.png"), b"outside")?;
            let description = extensions::describe(&folder)?;
            assert_eq!(description.name, case["expected_name"], "{}", case["name"]);
            let icon = description
                .icon
                .map(|path| {
                    path.strip_prefix(&folder)
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                })
                .transpose()?;
            assert_eq!(
                serde_json::to_value(icon)?,
                case["icon"],
                "{}",
                case["name"]
            );
            let ids: Vec<_> = extensions::installed(&root)?
                .into_iter()
                .map(|entry| entry.id)
                .collect();
            assert_eq!(serde_json::to_value(ids)?, case["installed_ids"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
