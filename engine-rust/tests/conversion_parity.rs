use numera_core::{
    conversion::{secret_intent, to_dashboard, to_engine},
    models::Profile,
};
use serde_json::Value;
mod common;

#[test]
fn conversion_text_boundaries_match_original_profiles_and_roundtrips()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("conversion-text")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import convert
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/conversion.json').read_text(encoding='utf-8'))
base=next(case['input'] for case in fixture['cases'] if case['operation']=='to-engine')
base=dict(base,id='conversion-text')
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
rows=[]
def add(label,changes): rows.append((label,dict(copy.deepcopy(base),**changes)))
line_markers=[chr(value) for value in [0xd,0xa,0xb,0xc,0x1c,0x1d,0x1e,0x85,
                                      0x2028,0x2029,0x1f,0x9,0x200b,0x58]]+['\r\n']
entries={'fontList':[' First α ',' Second 日本語 '],
         'extensions':[' first-extension ',' second-extension '],
         'launchArgs':[' --first=α ',' --second=日本語 '],
         'startupUrls':[' one.test/path?q=red,blue ',' https://two.test '],
         'bookmarks':[' First α | https://one.test/a,b ',' https://two.test ']}
for marker in line_markers:
    add('string-'+repr(marker),{key:marker.join(value) for key,value in entries.items()})
    add('array-'+repr(marker),{key:[marker.join(value)] for key,value in entries.items()})
whitespace=[*range(9,14),*range(0x1c,0x20),0x20,0x85,0xa0,0x1680,
            *range(0x2000,0x200b),0x2028,0x2029,0x202f,0x205f,0x3000]
for value in whitespace+[0x200b,0x58,0,0xfeff]:
    marker=chr(value); wrap=lambda value:marker+value+marker
    changes={key:[wrap(items[0].strip())] for key,items in entries.items()}
    changes.update(webglRenderer=wrap('Renderer α'),webglVendor=wrap('Vendor 日本語'),
                   dns=wrap('https://resolver.test/dns-query'))
    for field in ('webrtc','canvas','audio','clientRects','webglImage','mediaDevices','fonts','geo'):
        changes[field]=wrap(' Altered ')
    changes['bookmarks']=[wrap(' First α ')+ '|' +wrap(' https://one.test/a,b ')]
    changes['startupUrls']=wrap('one.test,two.test')
    add('trim-'+hex(value),changes)
for value in (None,False,0,[],{},[' α ',17,None,False,{},'']):
    add('nontext-'+str(len(rows)),{key:value for key in entries})
observed=[]
with run_log('conversion-text-oracle',parent/'logs') as log:
    for label,record in rows:
        before=copy.deepcopy(record)
        profile=convert.to_engine_profile(record)
        observed.append({'label':label,'input':before,'profile':profile.to_dict(),
                         'dashboard':convert.to_dashboard_profile(profile)})
        assert record==before,'Original conversion mutated its input'
    log.info('Observed conversion text cases=%s',len(observed))
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
    assert_eq!(observed.exit, 0, "Original conversion text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 69, "Conversion text observations missing");
    for case in &cases {
        let profile = to_engine(&case["input"], 1700000000.0)?;
        assert_eq!(
            serde_json::to_value(&profile)?,
            case["profile"],
            "Conversion text boundaries differ: {}",
            case["label"]
        );
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Conversion text roundtrip differs: {}",
            case["label"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn profile_shapes_and_secret_intent_match_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/conversion.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing conversion source cases")?;
    assert!(!cases.is_empty(), "Conversion oracle must not be empty");
    for case in cases {
        let actual = match case["operation"]
            .as_str()
            .ok_or("Missing conversion operation")?
        {
            "to-engine" => serde_json::to_value(to_engine(&case["input"], 1700000000.0)?)?,
            "to-dashboard" => to_dashboard(&Profile::from_record(&case["input"])?),
            "secret-intent" => Value::String(secret_intent(&case["input"]).into()),
            _ => return Err("Unknown conversion operation".into()),
        };
        assert_eq!(
            actual, case["expected"],
            "Conversion source case {}",
            case["name"]
        );
    }
    Ok(())
}
