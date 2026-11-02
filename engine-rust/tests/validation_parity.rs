use numera_core::validation::profile_problems;
use serde_json::Value;
mod common;
#[path = "validation_parity/contract.rs"]
mod contract;

#[test]
fn packed_screen_integer_grammar_matches_original() -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{http::profile_fields, ownership::capture};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("screen-grammar")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import write_contract
from numera.server.routes_profiles import check_fields
from run_logging import run_log
with run_log('screen-grammar-oracle',root/'logs') as log:
    values=['1_920x1_080','١٩٢٠x١٠٨٠','۱۹۲۰x۱۰۸۰','１９２０x１０８０',
            '𝟙𝟡𝟚𝟘x𝟙𝟘𝟠𝟘','+١_٩٢٠X+١٠٨٠','1920x1080',
            '+1920x+1080','001920x001080',' 1920 x 1080 ',
            '\t1920\nx\r1080\t',' 1920 x 1080 ',
            '1x1','65536x65536','1x65536','65536x1','6_5536x6_5536',
            '0x1080','1920x0','65537x1080','1920x65537',
            '-1x1080','1920x-1080','-0x1080','1__920x1080',
            '_1920x1080','1920_x1080','1920x_1080','1920x1080_',
            '+x1080','1920x+','+ 1920x1080','1920x1080x1',
            '1920','x1080','1920x','1.0x1080','1e3x1080',
            '²x1080','Ⅰx1080','18446744073709551616x1080','',None,17,True,[]]
    rows=[]
    for value in values:
        record={'screen':value}
        rows.append({'input':record,
                     'core':write_contract.profile_problems(record,kind=write_contract.DASHBOARD_PATCH),
                     'http':check_fields(record,where='profile')})
    log.info('Observed packed screen grammar cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original packed screen observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 46, "Packed screen observations missing");
    for case in cases {
        assert_eq!(
            serde_json::to_value(profile_problems(&case["input"], "dashboard patch")?)?,
            case["core"],
            "Packed screen integer grammar differs: {}",
            case["input"]
        );
        assert_eq!(
            serde_json::to_value(profile_fields::check(&case["input"], "profile", &[])?)?,
            case["http"],
            "HTTP packed screen integer grammar differs: {}",
            case["input"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn ascii_refusal_representation_matches_original_without_exposing_secret_values()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("ascii-refusal-representation")?;
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
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.write_contract import profile_problems,DASHBOARD_PATCH
from run_logging import run_log
values=[chr(code) for code in range(128)]
values += ["'",'"',"'\"",'\\',"a'\"\\\n\t\r\x00z",'x'*35+'\x1b'+'y'*20]
observed=[]
with run_log('ascii-refusal-representation-oracle',parent/'logs') as log:
    for value in values:
        for name in [[value],{'nested':[value,True,None]}]:
            record={'name':name,'proxy':{'password':['private-control-'+value],
                                       'pass':{'private-control':value}}}
            before=json.dumps(record,ensure_ascii=True)
            expected=profile_problems(record,kind=DASHBOARD_PATCH)
            assert json.dumps(record,ensure_ascii=True)==before
            assert len(expected)==3 and all('private-control' not in error for error in expected)
            observed.append({'input':record,'expected':expected})
    log.info('Observed ASCII refusal cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path())
        .env("PYTHONUTF8", "1");
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 524288,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original refusal observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 268, "ASCII refusal observations missing");
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        let actual = profile_problems(&input, "dashboard patch")?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "ASCII refusal representation differs: case={index}"
        );
        assert_eq!(input, case["input"], "Validation changed its input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn unicode_representation_matches_original_public_validation_and_argv()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{cli, http::profile_fields, ownership::capture};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("unicode-representation")?;
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
import json,sys,unicodedata
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import cli,mcp,write_contract
from numera.server.routes_profiles import check_fields
from run_logging import run_log
assert unicodedata.unidata_version=='15.1.0','Frozen Unicode observer version differs'
points=list(range(256))+[
    0x300,0x374,0x378,0x61c,0x1680,0x180e,0x2000,0x200b,0x200c,0x200d,
    0x2028,0x2029,0x202e,0x2060,0x2066,0x2069,0x20ac,0x3000,0xd7ff,
    0xe000,0xf8ff,0xf900,0xfdcf,0xfdd0,0xfdef,0xfdf0,0xfe0f,0xfeff,
    0xfffd,0xfffe,0xffff,0x10000,0x1bca0,0x1d173,0x1f40d,0x1f600,
    0x1fae8,0x1fae9,0x2ebf0,0x31350,0xe0001,0xe0020,0xe007f,0xe0100,
    0xeffff,0xf0000,0xffffd,0xffffe,0xfffff,0x100000,0x10fffd,0x10fffe,0x10ffff]
values=[chr(point) for point in points]
values += ["a'\"\\\n\t\r ‍\U000e0020z",'مرحبا 日本語 🐍',
           'a'*34+'‍'+'b'*8,'a'*35+' '+'b'*8,
           'a'*30+'\U000e0020'+'b'*8,'a'*34+'🐍'+'b'*8]
path,parser,_=next(row for row in mcp._walk(cli.build_parser(),(),{}) if row[0]==('show',))
rows=[]
with run_log('unicode-representation-oracle',parent/'logs') as log:
    for value in values:
        record={'name':{'nested':[value,True,None],value:'tail'},
                'proxy':{'password':['private-control-'+value],
                         'pass':{'private-control':value}}}
        http={value:None}
        arguments={'ref':[{'nested':[value,True,None],value:'tail'},value]}
        before=json.dumps([record,http,arguments],ensure_ascii=True)
        core=write_contract.profile_problems(record,kind=write_contract.DASHBOARD_PATCH)
        assert len(core)==3 and all('private-control' not in error for error in core)
        rows.append({'input':record,'core':core,'http_input':http,
                     'http':check_fields(http,where='profile'),'arguments':arguments,
                     'argv':mcp._argv(path,parser,arguments)})
        assert json.dumps([record,http,arguments],ensure_ascii=True)==before
    log.info('Observed Unicode representation cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path())
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 2 * 1024 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original Unicode representation observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(
        cases.len(),
        315,
        "Unicode representation observations missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        assert_eq!(
            serde_json::to_value(profile_problems(&input, "dashboard patch")?)?,
            case["core"],
            "Unicode representation differs: core case={index}"
        );
        assert_eq!(
            serde_json::to_value(profile_fields::check(&case["http_input"], "profile", &[])?)?,
            case["http"],
            "Unicode representation differs: HTTP case={index}"
        );
        assert_eq!(
            serde_json::to_value(cli::reflection::argv(
                "show",
                case["arguments"]
                    .as_object()
                    .ok_or("Source arguments missing")?
            )?)?,
            case["argv"],
            "Unicode representation differs: argv case={index}"
        );
        assert_eq!(input, case["input"], "Validation changed its input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn uncoerced_write_validation_matches_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/validation.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing write validation cases")?;
    assert!(
        !cases.is_empty(),
        "Write validation oracle must not be empty"
    );
    for case in cases {
        let actual = profile_problems(
            &case["input"],
            case["kind"]
                .as_str()
                .ok_or("Missing declared profile shape")?,
        )?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "Write validation {}",
            case["name"]
        );
    }
    Ok(())
}
