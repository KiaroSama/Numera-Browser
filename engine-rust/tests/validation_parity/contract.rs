use super::common;
use numera_core::{
    models::Profile,
    ownership::capture,
    write_contract::{self, Input},
};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn public_write_contract_shapes_contexts_and_refusals_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("write-contract-boundary")?;
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
from numera import write_contract as contract
from numera.models import Profile
from run_logging import run_log
base=json.loads((repository/'engine-rust/tests/fixtures/repo-write.json').read_text(encoding='utf-8'))['cases'][0]['initial']['profiles']
invalid=copy.deepcopy(base);invalid['name']=77;invalid['tags']=[None]
invalid_model=copy.deepcopy(base);invalid_model['name']=77;invalid_model['fingerprint']['screen_width']=0
values=[None,True,17,'text',[],{}, {'name':'valid patch'}, {'fingerprint':None},
        {'fingerprint':17}, {'fingerprint':{}}, {'name':77,'cores':0,'proxy':{'pass':[]}},base,invalid]
rows=[]
def observe(call):
    try:return {'ok':call()}
    except (ValueError,TypeError) as exc:return {'error':type(exc).__name__,'message':str(exc)}
with run_log('write-contract-boundary-oracle',parent/'logs') as log:
    for model,records in ((False,values),(True,[base,invalid_model])):
        for record in records:
            for kind in (None,'engine object','engine record','dashboard patch','unknown',''):
                for where in ('profile','profiles[4]','caller: profile: retained'):
                    value=Profile.from_dict(copy.deepcopy(record)) if model else copy.deepcopy(record)
                    before=value.to_dict() if model else copy.deepcopy(value)
                    intent=value.proxy.password_intent if model and value.proxy else None
                    rows.append({'input':record,'model':model,'kind':kind,'where':where,
                                 'shape':observe(lambda:contract.shape(value,kind=kind)),
                                 'problems':observe(lambda:contract.profile_problems(value,kind=kind,where=where)),
                                 'require':observe(lambda:contract.require(value,kind=kind,where=where)),
                                 'flat':observe(lambda:contract.require_flat_numbers(value,where=where))})
                    assert (value.to_dict() if model else value)==before,'Original validation changed input'
                    assert (value.proxy.password_intent if model and value.proxy else None)==intent
    log.info('Observed write-contract cases=%s',len(rows))
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
    assert_eq!(observed.exit, 0, "Original write-contract observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 270, "Write-contract observations missing");
    fn outcome<T: serde::Serialize>(result: Result<T, write_contract::Error>) -> Value {
        match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":match &error {
                write_contract::Error::Refused(_) => "WriteRefused",
                write_contract::Error::UnknownShape(_) => "ValueError",
                write_contract::Error::Policy(_) => "PolicyError",
            },"message":error.to_string()}),
        }
    }
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let model = if case["model"] == true {
            Some(Profile::from_record(&raw)?)
        } else {
            None
        };
        let before = model.as_ref().map(serde_json::to_value).transpose()?;
        let intent = model
            .as_ref()
            .and_then(|profile| profile.proxy.as_ref())
            .map(|proxy| proxy.password_intent());
        let input = model.as_ref().map_or(Input::Json(&raw), Input::Profile);
        let kind = case["kind"].as_str();
        let context = case["where"].as_str().ok_or("Context missing")?;
        assert_eq!(
            outcome(write_contract::shape(input, kind)),
            case["shape"],
            "Write shape differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::profile_problems(input, context, kind)),
            case["problems"],
            "Contextual validation differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::require(input, context, kind)),
            case["require"],
            "Typed write refusal differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::require_flat_numbers(input, context)),
            case["flat"],
            "Flat write refusal differs: {index}"
        );
        assert_eq!(raw, case["input"], "Raw validation input changed");
        assert_eq!(
            model.as_ref().map(serde_json::to_value).transpose()?,
            before,
            "Model validation input changed"
        );
        assert_eq!(
            model
                .as_ref()
                .and_then(|profile| profile.proxy.as_ref())
                .map(|proxy| proxy.password_intent()),
            intent,
            "Validation changed credential intent"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
