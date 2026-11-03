use super::common;
use numera_core::{import_format, importers, ownership::capture};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn importer_lexical_profiles_match_original_without_changing_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("importer-lexical")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys,time
from pathlib import Path
from unittest.mock import patch
time.time=lambda:1000.0
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import importers,write_contract
from numera.convert import to_dashboard_profile
from run_logging import run_log
values=[None,True,False,0,8,8.9,-5,4096,4097,'','8 cores','N/A','8.9','8e0',
        '1_2','١٢','१२','１２','\u001c12\u001f',' 12 ','1__2','_12','12_',
        'inf','-Infinity','NaN','1e309','9'*400,[],{}]
resolutions=['','1920x1080','1920 x 1080','١٩٢٠x١٠٨٠','１_９２０x１_０８０',
             ' 1920x1080 ','+1920x+1080','1920x1080x24',
             '1920.0x1080','1__920x1080','0x0','-1x-2','1920','x','1920X1080']
versions=['','Chrome/128','Chrome/128.1.2.3.4','Chrome/x Chrome/120.1',
          'Chrome/١٢٨.٢','Chrome/１２８.０.１','Chrome/² Chrome/121',
          'Chrome/128.x.2','Chrome/128..2','chrome/129','Chrome/. Chrome/130',
          'Chrome/128a.2','Chrome/128.١.２.3','Chrome/128/Chrome/129']
cases=[]
for field in ('cores','memory'):
    cases.extend({field:value} for value in values)
cases.extend({'screen':value} for value in resolutions)
cases.extend({'user_agent':value} for value in versions)
rows=[]
original=importers.generate
with run_log('importer-lexical-oracle',parent/'logs') as log:
    for delta in cases:
        raw={'name':'lexical','os':'Windows',**delta};before=copy.deepcopy(raw)
        with patch.object(importers,'generate',side_effect=lambda **kw:original(seed=42,**kw)),patch('time.time',return_value=1000.0):
            profile,issues=importers.profile_from_export(raw)
        record=profile.to_dict();record['id']='generated'
        try:
            write_contract.require(profile)
            outcome={'ok':record}
        except write_contract.WriteRefused as error:
            outcome={'error':str(error)}
        rows.append({'input':before,'model':outcome,'dashboard':to_dashboard_profile(profile),'issues':issues})
        assert raw==before,'Original importer changed input'
    log.info('Observed importer lexical cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original importer observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 89, "Importer observations missing");
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let (dashboard, issues) = importers::profile(&raw, 42, 1000.0)?;
        assert_eq!(
            dashboard, case["dashboard"],
            "Imported lexical dashboard differs: {index}"
        );
        assert_eq!(
            json!(issues),
            case["issues"],
            "Imported lexical issues differ: {index}"
        );
        let outcome = match importers::model(&raw, 42, 1000.0) {
            Ok((profile, issues)) => {
                assert_eq!(profile.id.len(), 12);
                assert!(
                    profile
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                );
                let mut record = serde_json::to_value(profile)?;
                record["id"] = "generated".into();
                assert_eq!(json!(issues), case["issues"]);
                json!({"ok":record})
            }
            Err(error) => json!({"error":error}),
        };
        assert_eq!(
            outcome, case["model"],
            "Imported lexical model differs: {index}"
        );
        assert_eq!(raw, case["input"], "Native importer changed input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_envelope_versions_match_original_exact_numeric_guidance()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("import-envelope-versions")?;
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
import copy,json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.importers import native_export
from run_logging import run_log
rows=[]
with run_log('import-envelope-oracle',parent/'logs') as log:
    for version in (None,False,True,0,1,1.0,2,2.0,-1,2**63,2**64,10**100,-10**100,'1','next',[],{}):
        raw={'numeraProfiles':version,'profiles':[{'name':'preserved'}]};before=copy.deepcopy(raw)
        try:outcome={'ok':native_export(raw)}
        except ValueError as error:outcome={'error':str(error)}
        rows.append({'input':before,'outcome':outcome})
        assert raw==before,'Original envelope parser changed input'
    log.info('Observed native version cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path().join("source"))
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original envelope observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 17, "Native version observations missing");
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let actual = match import_format::native(&raw) {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        };
        assert_eq!(
            actual, case["outcome"],
            "Native export version guidance differs: {index}"
        );
        assert_eq!(raw, case["input"], "Native envelope parser changed input");
    }
    temporary.cleanup()?;
    Ok(())
}
