use super::common;
use numera_core::{importers, ownership::capture};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

fn observations(mode: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("importer-tags")?;
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
repository,parent=map(Path,sys.argv[1:3])
mode=sys.argv[3]
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import importers,write_contract
from numera.convert import to_dashboard_profile
from run_logging import run_log
cases=[]
if mode=='tags':
    whitespace=[9,10,11,12,13,28,29,30,31,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288]
    for code in whitespace:
        gap=chr(code)
        cases.append({'tags':gap+'alpha'+gap+','+gap+','+gap+'beta'+gap})
    for group in (None,False,True,0,7,'',"a'b",['x',True,None],{'value':[False,None]}):
        cases.append({'tags':'keep','group':group})
else:
    bad={'name':7,'notes':7,'tags':[7],'timezone':{'unexpected':'zone'},
         'webgl_vendor':7,'webgl_renderer':False,'screen':'0x0','cores':0,'memory':4096,
         'proxy':{'type':'https','host':'proxy.test','port':8080,'user':7,'pass':7,'country':7}}
    cases=[bad,{**bad,'name':['invalid']},{**bad,'notes':{'invalid':True}},
           {**bad,'tags':[None],'proxy':{**bad['proxy'],'pass':{'secret-marker':True}}}]
rows=[]
original=importers.generate
with run_log('importer-tag-oracle',parent/'logs') as log:
    for delta in cases:
        raw={'name':'tag-fixture','os':'Windows',**delta}
        before=copy.deepcopy(raw)
        with patch.object(importers,'generate',side_effect=lambda **kw:original(seed=42,**kw)):
            profile,issues=importers.profile_from_export(raw)
        record=profile.to_dict()
        record['id']='generated'
        try:
            write_contract.require(profile)
            result={'ok':record}
        except write_contract.WriteRefused as error:
            result={'error':str(error)}
        if mode=='refusals':
            assert len(write_contract.profile_problems(profile))>8,'Fixture needs a complete refusal'
            assert 'secret-marker' not in result['error'],'Secret value appeared in refusal'
        rows.append({'input':before,'dashboard':to_dashboard_profile(profile),'issues':issues,'model':result})
        assert raw==before,'Original importer changed input'
    log.info('Observed importer cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).arg(mode).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 524288,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original importer tag observer failed");
    let rows = serde_json::from_slice(&observed.stdout)?;
    temporary.cleanup()?;
    Ok(rows)
}

fn compare(rows: &[Value], symptom: &str) -> Result<(), Box<dyn std::error::Error>> {
    for (index, case) in rows.iter().enumerate() {
        let raw = case["input"].clone();
        let (dashboard, issues) = importers::profile(&raw, 42, 1000.0)?;
        assert_eq!(dashboard, case["dashboard"], "{symptom}: {index}/dashboard");
        assert_eq!(json!(issues), case["issues"], "{symptom}: {index}/issues");
        let model = match importers::model(&raw, 42, 1000.0) {
            Ok((profile, issues)) => {
                assert_eq!(profile.id.len(), 12);
                assert!(
                    profile
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                );
                assert_eq!(json!(issues), case["issues"]);
                let mut record = serde_json::to_value(profile)?;
                record["id"] = "generated".into();
                json!({"ok":record})
            }
            Err(error) => json!({"error":error}),
        };
        assert_eq!(model, case["model"], "{symptom}: {index}/model");
        assert_eq!(raw, case["input"], "Native importer changed input");
    }
    Ok(())
}

#[test]
fn importer_tags_match_original_whitespace_and_group_display()
-> Result<(), Box<dyn std::error::Error>> {
    let rows = observations("tags")?;
    assert_eq!(rows.len(), 38, "Importer tag observations missing");
    compare(&rows, "Imported tag display differs")
}

#[test]
fn importer_refusals_retain_all_original_ordered_problems() -> Result<(), Box<dyn std::error::Error>>
{
    let rows = observations("refusals")?;
    assert_eq!(rows.len(), 4, "Complete importer refusals missing");
    compare(&rows, "Complete importer refusal differs")
}
