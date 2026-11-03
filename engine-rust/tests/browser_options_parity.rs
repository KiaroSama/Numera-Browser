use numera_core::{browser::options, models::Profile};
use serde_json::Value;
mod common;

#[test]
fn dns_options_match_original_whitespace_and_refusals() -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("dns-options")?;
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
from numera.fingerprint import generate
from numera.models import Profile
from numera.launch_options import _dns_args
from run_logging import run_log
whitespace=[9,10,11,12,13,28,29,30,31,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288]
values=[chr(code)+value+chr(code) for code in whitespace
        for value in ('','9.9.9.9','https://resolver.test/dns-query','invalid')]
values+=['','1.1.1.1','1.0.0.1','8.8.8.8','8.8.4.4','149.112.112.112',
         '94.140.14.14','208.67.222.222','HTTPS://resolver.test/dns-query',"a'b"]
profile=Profile(id='dns-fixture',name='DNS fixture',fingerprint=generate(seed=42),created_at=1000.0,updated_at=1000.0)
rows=[]
with run_log('dns-options-oracle',root/'logs') as log:
    for dns in values:
        profile.dns=dns
        before=copy.deepcopy(profile.to_dict())
        try:result={'args':_dns_args(profile)}
        except ValueError as error:result={'error':str(error)}
        rows.append({'input':before,'result':result})
        assert profile.to_dict()==before,'Original DNS options changed input'
    log.info('Observed DNS option cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original DNS option observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 126, "DNS option observations missing");
    for (index, row) in rows.iter().enumerate() {
        let profile = Profile::from_record(&row["input"])?;
        let before = serde_json::to_value(&profile)?;
        for backend in ["playwright", "patchright", "cloak", "camoufox"] {
            let actual = options::build(&profile, backend, "user-data/a", true, &[], &[], None);
            let refuses = backend != "camoufox" && row["result"]["error"].is_string();
            assert_eq!(
                actual.is_err(),
                refuses,
                "DNS launch options differ: {index}/{backend}/admission"
            );
            if refuses {
                let error = actual.expect_err("DNS launch options differ: refusal missing");
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput,
                    "DNS launch options differ: {index}/{backend}/error kind"
                );
                assert_eq!(
                    error.to_string(),
                    row["result"]["error"].as_str().ok_or("Refusal missing")?,
                    "DNS launch options differ: {index}/{backend}/refusal"
                );
                continue;
            }
            let actual = actual?;
            let mut baseline = profile.clone();
            baseline.dns.clear();
            let mut expected =
                options::build(&baseline, backend, "user-data/a", true, &[], &[], None)?;
            if backend != "camoufox" {
                let dns: Vec<Value> = serde_json::from_value(row["result"]["args"].clone())?;
                let arguments = expected["args"]
                    .as_array_mut()
                    .ok_or("Launch args missing")?;
                let position = if backend == "cloak" { 4 } else { 5 };
                arguments.splice(position..position, dns);
            }
            assert_eq!(
                actual, expected,
                "DNS launch options differ: {index}/{backend}"
            );
        }
        assert_eq!(
            serde_json::to_value(&profile)?,
            before,
            "Native DNS options changed input"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn effective_sdk_options_match_retained_launch_boundary() -> Result<(), Box<dyn std::error::Error>>
{
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-options.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Browser option cases missing")?;
    assert_eq!(cases.len(), 40);
    for case in cases {
        let profile = Profile::from_record(&case["input"])?;
        let extras: Vec<String> = serde_json::from_value(case["extra_args"].clone())?;
        let actual = options::build(
            &profile,
            case["backend"].as_str().ok_or("Backend missing")?,
            "user-data/a",
            case["headless"].as_bool().ok_or("Headless missing")?,
            &extras,
            &[],
            None,
        )?;
        assert_eq!(actual, case["expected"], "SDK options {}", case["name"]);
    }
    Ok(())
}
