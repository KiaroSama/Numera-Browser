use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::fingerprint_routes::router;
use serde_json::Value;
use std::time::Duration;
use tower::ServiceExt;
mod common;

#[test]
fn public_dashboard_coherence_matches_original_ordered_issues_and_http()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{conversion::dashboard_coherence, ownership::capture};
    use std::{path::Path, process::Command};

    let temporary = common::TempDirectory::new("dashboard-coherence")?;
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
from numera.convert import dashboard_coherence
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/fingerprint-routes.json').read_text(encoding='utf-8'))
rows=[(case['name'],case['input']) for case in fixture['cases'] if isinstance(case['input'],dict)]
for os in ('Windows','macOS','Linux','Android','iOS'):
    rows.append(('os-'+os,{'os':os}))
    rows.append(('invalid-'+os,{'os':os,'screen':'9000x9000','maxTouchPoints':25,
                              'devicePixelRatio':5,'mediaVideo':-1,'mediaAudio':-1,
                              'webglRenderer':'alien','fontList':['Zulu α','Alpha 日本語'],
                              'timezone':'unknown'}))
for record in ({'id':'explicit'}, {'screen':'not a screen'}, {'locale':'not-a-locale'},
               {'cores':'3.5'}, {'cores':3.5}, {'cores':False},
               {'proxy':{'host':'fixture.test','type':'SOCKS4'}},
               {'proxy':{'host':'fixture.test','type':'INVALID'},'screen':'9000x9000'},
               {'os':'Android','maxTouchPoints':0,'mediaVideo':0,'mediaAudio':0},
               {'fontList':['Zulu','Alpha','Beta','Alpha']}):
    rows.append(('extra-'+str(len(rows)),record))
observed=[]
with run_log('dashboard-coherence-oracle',parent/'logs') as log:
    for label,record in rows:
        before=copy.deepcopy(record)
        issues=dashboard_coherence(record)
        assert record==before,'Original coherence mutated its input'
        observed.append({'label':label,'input':record,'issues':issues})
    assert any(len(row['issues'])>=5 for row in observed),'Ordered issue control missing'
    assert any(row['issues'] and row['issues'][0].startswith('could not validate: ')
               for row in observed),'Conversion refusal control missing'
    log.info('Observed public coherence cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original coherence observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 30, "Public coherence observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let app = router(temporary.path().join("native"), "fixture-api");
        for case in &cases {
            let input = case["input"].clone();
            assert_eq!(
                serde_json::to_value(dashboard_coherence(&input, 1700000000.0))?,
                case["issues"],
                "Public dashboard coherence differs: {}",
                case["label"]
            );
            assert_eq!(input, case["input"], "Coherence mutated its input");
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(
                    Request::post("/api/fingerprint/validate")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&input)?))?,
                ),
            )
            .await??;
            assert_eq!(
                response.status(),
                200,
                "Coherence HTTP status differs: {}",
                case["label"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                serde_json::json!({"issues":case["issues"]}),
                "Coherence HTTP result differs: {}",
                case["label"]
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fingerprint_http_validation_uses_original_tolerant_conversion()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/fingerprint-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Fingerprint HTTP cases missing")?;
        assert!(!cases.is_empty());
        let temporary = common::TempDirectory::new("fingerprint-route")?;
        let app = router(temporary.path().to_path_buf(), "fixture-api");
        for case in cases {
            let request = Request::post("/api/fingerprint/validate")
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&case["input"])?))?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(request))
                    .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                case["body"],
                "{}",
                case["name"]
            );
        }
        temporary.cleanup()?;
        Ok(())
    })
}
