use super::common;
use numera_core::{bulk, models::Profile, ownership::capture};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn selection_matches_original_complete_profile_membership() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("bulk-selection")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import bulk
from numera.fingerprint import generate
from numera.models import Profile
from run_logging import run_log
base=Profile(id='same-id',name='Shared',fingerprint=generate(seed=42),tags=['keep'],created_at=1000.0,updated_at=1000.0).to_dict()
variants=[{}, {'notes':'other'}, {'name':'Other'}, {'tags':['other']},
          {'startup_urls':['about:blank']}, {'extensions':['folder']},
          {'bookmarks':[{'url':'https://example.test','name':'Entry'}]},
          {'engine':'camoufox'}, {'created_at':1001.0}, {'updated_at':1001.0},
          {'name':1}, {'name':True}, {'name':1.0},
          {'bookmarks':[{'value':1}]}, {'bookmarks':[{'value':1.0}]},
          {'proxy':{'server':'http://proxy.test:8080','username':None,'password':'public-fixture','country':None}}]
rows=[]
with run_log('bulk-selection-oracle',root/'logs') as log:
    for delta in variants:
        first=copy.deepcopy(base)
        second={**copy.deepcopy(base),**delta}
        third={**copy.deepcopy(second),'id':'other-id'}
        for options in ({'refs':['same-id']},{'refs':['same-id'],'tag':'keep'},
                        {'refs':['same-id','same-id','missing']},{'everything':True},{}):
            raw=[first,second,third]
            before=copy.deepcopy(raw)
            profiles=[Profile.from_dict(copy.deepcopy(record)) for record in raw]
            selected=bulk.select(profiles,**options)
            rows.append({'input':before,'options':options,'selected':[p.to_dict() for p in selected]})
            assert [p.to_dict() for p in profiles]==before,'Original selection changed profiles'
    for left,right in ((1,1.0),(True,1),(9007199254740993,9007199254740992.0)):
        raw=[{**copy.deepcopy(base),'name':left},{**copy.deepcopy(base),'name':right}]
        profiles=[Profile.from_dict(copy.deepcopy(record)) for record in raw]
        rows.append({'input':raw,'options':{'refs':['same-id']},
                     'selected':[p.to_dict() for p in bulk.select(profiles,refs=['same-id'])]})
        assert [p.to_dict() for p in profiles]==raw,'Original selection changed profiles'
    log.info('Observed bulk selection cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 2097152,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original bulk selection observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 83, "Bulk selection observations missing");
    for (index, row) in rows.iter().enumerate() {
        let profiles = row["input"]
            .as_array()
            .ok_or("Selection profiles missing")?
            .iter()
            .map(Profile::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let before = serde_json::to_value(&profiles)?;
        let references: Vec<String> =
            serde_json::from_value(row["options"].get("refs").cloned().unwrap_or(json!([])))?;
        let selected = bulk::select(
            &profiles,
            &references,
            row["options"]["tag"].as_str(),
            row["options"]["everything"] == true,
        );
        assert_eq!(
            serde_json::to_value(selected)?,
            row["selected"],
            "Bulk profile selection differs: {index}"
        );
        assert_eq!(
            serde_json::to_value(&profiles)?,
            before,
            "Selection changed input"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
