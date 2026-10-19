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
fn reverse_proxy_conversion_matches_original_without_canonicalizing_records()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("conversion-proxy")?;
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
from numera.convert import to_dashboard_profile
from numera.models import Profile
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/conversion.json').read_text(encoding='utf-8'))
base=next(case['input'] for case in fixture['cases'] if case['operation']=='to-dashboard')
base=dict(base,id='conversion-proxy')
rows=[]
def add(label,server,**credentials):
    proxy=dict(server=server,username='user α',password='synthetic-value 😀',country='ZZ')
    proxy.update(credentials)
    rows.append((label,proxy))
for scheme in ('socks','socks5h','http','https','socks4','socks5','unknown',''):
    for spelling in dict.fromkeys((scheme,scheme.upper(),scheme.capitalize())):
        add('scheme-'+str(len(rows)),spelling+'://Proxy.Example:0080')
whitespace=[*range(9,14),*range(0x1c,0x20),0x20,0x85,0xa0,0x1680,
            *range(0x2000,0x200b),0x2028,0x2029,0x202f,0x205f,0x3000]
for value in whitespace+[0x200b,0,0xfeff]:
    marker=chr(value)
    add('boundary-'+hex(value),marker+'SoCkS5H://[2001:db8::1]:01080'+marker)
    add('only-'+hex(value),marker)
for server in ('','proxy.test','proxy.test:','proxy.test:port','proxy.test:1:2',
               ':1080',':','HTTP://','://host:port','http://host:80://tail',
               '[2001:db8::1]:1080','[2001:db8::1]','2001:db8::1',
               '[2001:db8::1','[]:80','[::1]suffix','[::1]:80:tail',
               'http://host/path?q=1#fragment','http://user:pass@host:80',
               'socks5://日本語.test:080','http:// host : 0080 '):
    add('host-'+str(len(rows)),server)
for value in (None,'',' value α 😀 ','enc:v2:synthetic-envelope'):
    add('credentials-'+str(len(rows)),'https://Proxy.Example:00443',
        username=value,password=value,country=value)
rows.append(('absent-proxy',None))
observed=[]
with run_log('conversion-proxy-oracle',parent/'logs') as log:
    for label,proxy in rows:
        record=dict(copy.deepcopy(base),proxy=proxy)
        source_input=copy.deepcopy(record)
        profile=Profile.from_dict(record)
        before=copy.deepcopy(profile.to_dict())
        dashboard=to_dashboard_profile(profile)
        assert profile.to_dict()==before,'Original reverse conversion mutated its profile'
        assert record==source_input,'Original profile read mutated its input'
        observed.append({'label':label,'proxy':proxy,'dashboard':dashboard})
    log.info('Observed reverse proxy conversion cases=%s',len(observed))
    print(json.dumps({'base':base,'cases':observed},ensure_ascii=True))
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
    assert_eq!(observed.exit, 0, "Original reverse proxy observer failed");
    let oracle: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = oracle["cases"].as_array().ok_or("Proxy cases missing")?;
    assert_eq!(cases.len(), 112, "Reverse proxy observations missing");
    for case in cases {
        let mut record = oracle["base"].clone();
        record["proxy"] = case["proxy"].clone();
        let profile = Profile::from_record(&record)?;
        let before = serde_json::to_value(&profile)?;
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Reverse proxy conversion differs: {}",
            case["label"]
        );
        assert_eq!(
            serde_json::to_value(&profile)?,
            before,
            "Reverse conversion mutated its profile"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn integer_conversion_matches_original_grammar_defaults_and_seed_magnitude()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("conversion-integer")?;
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
base=dict(base,id='conversion-integer')
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
rows=[]
def add(label,value,seed_only=False):
    changes={'seed':value}
    if not seed_only:
        changes.update({key:value for key in ('cores','mediaVideo','mediaAudio','mediaSpeakers','maxTouchPoints')})
    rows.append((label,changes))
values=['3.5','1e2','-3.5','+1_024','١٢','۱۲','１２','𝟙𝟚','1__2','_12','12_',
        '+_12','+ 12','0x10','²','',None,True,False,[],{},0,12,-12,3.9,-3.9,
        9007199254740993,-9007199254740993,1e18,-1e18,'-000','+00012']
for index,value in enumerate(values):
    add('value-'+str(index),value)
whitespace=[*range(9,14),*range(0x1c,0x20),0x20,0x85,0xa0,0x1680,
            *range(0x2000,0x200b),0x2028,0x2029,0x202f,0x205f,0x3000]
for scalar in whitespace+[0x200b,0,0xfeff]:
    marker=chr(scalar)
    add('boundary-'+hex(scalar),marker+'+١_٢'+marker)
for value in ('NaN','Infinity','-inf','9'*4301,'0'*4301):
    add('invalid-'+str(len(rows)),value)
for value in (2**80+1,-(2**80+1),str(2**80+1),'-'+str(2**80+1),
              '+000'+str(2**80+1),'9'*4300,'0'*4299+'1','1_'*4299+'1',
              1e100,-1e100,1.7976931348623157e308):
    add('seed-'+str(len(rows)),value,True)
observed=[]
with run_log('conversion-integer-oracle',parent/'logs') as log:
    for label,changes in rows:
        record=dict(copy.deepcopy(base),**changes)
        before=copy.deepcopy(record)
        profile=convert.to_engine_profile(record)
        observed.append({'label':label,'changes':changes,'profile':profile.to_dict(),
                         'dashboard':convert.to_dashboard_profile(profile)})
        assert record==before,'Original integer conversion mutated its input'
    log.info('Observed integer conversion cases=%s',len(observed))
    print(json.dumps({'base':base,'cases':observed},ensure_ascii=True))
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
    assert_eq!(
        observed.exit, 0,
        "Original integer conversion observer failed"
    );
    let oracle: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = oracle["cases"].as_array().ok_or("Integer cases missing")?;
    assert_eq!(cases.len(), 80, "Integer conversion observations missing");
    for case in cases {
        let mut record = oracle["base"].clone();
        record
            .as_object_mut()
            .ok_or("Profile input missing")?
            .extend(
                case["changes"]
                    .as_object()
                    .ok_or("Numeric changes missing")?
                    .clone(),
            );
        let before = record.clone();
        let profile = to_engine(&record, 1700000000.0)?;
        assert_eq!(
            serde_json::to_value(&profile)?,
            case["profile"],
            "Integer conversion differs: {}",
            case["label"]
        );
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Integer conversion roundtrip differs: {}",
            case["label"]
        );
        assert_eq!(record, before, "Integer conversion mutated its input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fractional_conversion_matches_original_numeric_values_and_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("conversion-fractional")?;
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
base=dict(base,id='conversion-fractional')
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
rows=[]
def add(label,value,os='Windows'):
    rows.append((label,{'memory':value,'devicePixelRatio':value,'os':os}))
values=[' 0.25 ','١.٢٥','۱.۲۵','１.２５','𝟙.𝟚𝟝','1_0.2_5e-1','+2.','-.25',
        '1e2','1e100','-1e100',1e100,-1e100,2**80+1,9007199254740993,
        0.25,0.5,0,0.0,-0.0,'-0','0e3',None,True,False,[],{},'',
        '1__2','_1','1_','1_.0','1._0','1e_2','1e+_2','1,25','0x1p2',
        'NaN','+Infinity','-inf','1e9999','1e-9999','5e-324','1.7976931348623157e308']
for index,value in enumerate(values): add('value-'+str(index),value)
whitespace=[*range(9,14),*range(0x1c,0x20),0x20,0x85,0xa0,0x1680,
            *range(0x2000,0x200b),0x2028,0x2029,0x202f,0x205f,0x3000]
for scalar in whitespace+[0x200b,0,0xfeff]:
    marker=chr(scalar)
    add('boundary-'+hex(scalar),marker+'+١.٢_٥'+marker)
for value in (' 0.25 ','1e100','NaN',0,None,False):
    add('ios-'+str(len(rows)),value,'iOS')
observed=[]
with run_log('conversion-fractional-oracle',parent/'logs') as log:
    for label,changes in rows:
        record=dict(copy.deepcopy(base),**changes)
        before=copy.deepcopy(record)
        profile=convert.to_engine_profile(record)
        observed.append({'label':label,'changes':changes,'profile':profile.to_dict(),
                         'memory_float':isinstance(profile.fingerprint.device_memory,float),
                         'dashboard':convert.to_dashboard_profile(profile)})
        assert record==before,'Original fractional conversion mutated its input'
    log.info('Observed fractional conversion cases=%s',len(observed))
    print(json.dumps({'base':base,'cases':observed},ensure_ascii=True))
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
    assert_eq!(observed.exit, 0, "Original fractional observer failed");
    let oracle: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Fractional cases missing")?;
    assert_eq!(
        cases.len(),
        82,
        "Fractional conversion observations missing"
    );
    for case in cases {
        let mut record = oracle["base"].clone();
        record
            .as_object_mut()
            .ok_or("Profile input missing")?
            .extend(
                case["changes"]
                    .as_object()
                    .ok_or("Numeric changes missing")?
                    .clone(),
            );
        let before = record.clone();
        let profile = to_engine(&record, 1700000000.0)?;
        for (mut actual, mut expected, memory, ratio) in [
            (
                serde_json::to_value(&profile)?,
                case["profile"].clone(),
                "/fingerprint/device_memory",
                "/fingerprint/device_pixel_ratio",
            ),
            (
                to_dashboard(&profile),
                case["dashboard"].clone(),
                "/memory",
                "/devicePixelRatio",
            ),
        ] {
            for path in [ratio]
                .into_iter()
                .chain((case["memory_float"] == true).then_some(memory))
            {
                let bits = |record: &Value| {
                    record
                        .pointer(path)
                        .and_then(Value::as_f64)
                        .filter(|number| number.is_finite())
                        .map(f64::to_bits)
                };
                let observed = bits(&actual).ok_or("Converted numeric field is not finite")?;
                let original = bits(&expected).ok_or("Original numeric field is not finite")?;
                assert_eq!(
                    observed, original,
                    "Fractional conversion differs: {} {path}",
                    case["label"]
                );
                *actual
                    .pointer_mut(path)
                    .ok_or("Converted numeric field missing")? = Value::Null;
                *expected
                    .pointer_mut(path)
                    .ok_or("Original numeric field missing")? = Value::Null;
            }
            assert_eq!(
                actual, expected,
                "Fractional conversion differs: {}",
                case["label"]
            );
        }
        assert_eq!(record, before, "Fractional conversion mutated its input");
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
