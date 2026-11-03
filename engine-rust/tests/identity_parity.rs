use numera_core::{
    browser::hints,
    identity::{self, headers, metadata, validate},
    identity_script,
    models::Fingerprint,
    ownership::capture,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn shared_identity_helpers_match_original_versions_platforms_and_consumers()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("shared-identity-helpers")?;
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
from numera import fingerprint as identity
from numera.models import Fingerprint
from numera.stealth import build_init_script
from run_logging import run_log
base=json.loads((repository/'engine-rust/tests/fixtures/identity.json').read_text(encoding='utf-8'))['cases'][0]['input']
versions=['','128','128.1','128.1.2','128.1.2.3','128.1.2.3.4','.','...','.1.2.3','128.','00128.01.02.03','alpha.beta', ' v128 ', '128"quoted']
rows=[]
with run_log('shared-identity-helpers-oracle',parent/'logs') as log:
    variants=[]
    for system in ('windows','macos','linux','android','ios','iOS','unknown'):
        for version in versions:
            variants.append({**base,'os':system,'chrome_version':version})
    for system in ('windows','macos','android','ios','unknown'):
        for renderer in ('Apple M3','apple m3','Intel',''):
            variants.append({**base,'os':system,'webgl_renderer':renderer,'ch_platform':'""','os_version':'unknown-release','device_model':'custom model','is_mobile':True})
    for platform in ('','"','"""Windows""',' "Linux" ', 'custom platform'):
        variants.append({**base,'ch_platform':platform})
    for record in variants:
        before=copy.deepcopy(record);fp=Fingerprint.from_dict(record)
        script=build_init_script(fp)
        config=json.loads(script.split('  const cfg = ',1)[1].split(';\n',1)[0])
        rows.append({'input':record,'brands':identity.ua_brands(fp),'header':identity.sec_ch_ua(fp),
                     'sends':identity.sends_client_hints(fp),'full':identity._full_version(fp),
                     'metadata':identity.ua_metadata(fp),'headers':identity.client_hint_headers(fp),
                     'config':config})
        assert record==before,'Original identity helpers changed input'
    log.info('Observed shared identity cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original identity helper observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 123, "Shared identity observations missing");
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        let fingerprint = Fingerprint::from_record(&input)?;
        assert_eq!(
            identity::ua_brands(&fingerprint),
            case["brands"],
            "Shared brands differ: {index}"
        );
        assert_eq!(
            identity::sec_ch_ua(&fingerprint),
            case["header"].as_str().ok_or("Header missing")?,
            "Standalone hint header differs: {index}"
        );
        assert_eq!(
            identity::sends_client_hints(&fingerprint),
            case["sends"].as_bool().ok_or("Hint predicate missing")?,
            "Hint predicate differs: {index}"
        );
        assert_eq!(
            identity::full_version(&fingerprint),
            case["full"].as_str().ok_or("Full version missing")?,
            "Full version differs: {index}"
        );
        assert_eq!(
            metadata(&fingerprint)?,
            case["metadata"],
            "Shared metadata differs: {index}"
        );
        assert_eq!(
            headers(&fingerprint)?,
            case["headers"],
            "Shared headers differ: {index}"
        );
        assert_eq!(
            identity_script::config(&fingerprint)?,
            case["config"],
            "Shared page config differs: {index}"
        );
        let policy = hints::policy(&fingerprint)?;
        assert_eq!(
            policy["commands"][0]["payload"]["userAgentMetadata"], case["metadata"],
            "CDP metadata differs: {index}"
        );
        let sends = case["sends"].as_bool().ok_or("Hint predicate missing")?;
        assert_eq!(
            policy["strip_headers_prefixes"],
            if sends {
                json!([])
            } else {
                json!(["sec-ch-ua"])
            },
            "CDP hint omission differs: {index}"
        );
        assert_eq!(
            policy["commands"]
                .as_array()
                .ok_or("CDP commands missing")?
                .len(),
            if sends { 1 } else { 2 }
        );
        assert_eq!(
            serde_json::to_value(&fingerprint)?,
            input,
            "Native identity input changed: {index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn identity_diagnostics_and_hints_match_source() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/identity.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Missing identity contract fixtures")?;
    assert!(
        !cases.is_empty(),
        "Identity contract oracle must not be empty"
    );
    for case in cases {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        let actual = json!({"metadata": metadata(&fingerprint)?, "headers": headers(&fingerprint)?, "issues": validate(&fingerprint)?});
        assert_eq!(
            actual, case["expected"],
            "Identity contract {}",
            case["name"]
        );
    }
    Ok(())
}
