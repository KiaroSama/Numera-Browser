use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::{http::profile_routes::router, ownership::capture};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};
use tower::ServiceExt;

#[test]
fn divergent_profile_http_matches_original_refusal_kind_without_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::common::TempDirectory::new("divergence-http")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import base64,json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from fastapi.testclient import TestClient
from numera.server import create_app
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases']
rows=[]
with run_log('divergence-http-oracle',parent/'logs') as log:
    with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
        for index in [2,3]:
            root=parent/str(index)
            app=create_app(data_dir=str(root),start_scheduler=False)
            inputs={f'{side}/fixture00001.json':json.dumps(fixture[index][key],ensure_ascii=True).encode('utf-8')
                    for side,key in [('profiles','engine'),('dashboard','dashboard')]}
            inputs['config.json']=b'{}'
            for name,raw in inputs.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
            with TestClient(app,raise_server_exceptions=False) as client:
                response=client.post('/api/profiles/fixture00001/launch',json={},
                                     headers={'authorization':'Bearer fixture-api'})
            assert response.status_code==409 and response.json()['kind']=='ProfileDiverged','Original divergent HTTP seam missing'
            assert all((root/name).read_bytes()==raw for name,raw in inputs.items()),'Original divergent HTTP changed stored bytes'
            rows.append({'files':{name:base64.b64encode(raw).decode('ascii') for name,raw in inputs.items()},
                         'status':response.status_code,'body':response.json()})
    log.info('Observed divergent HTTP cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8", "1");
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original divergent HTTP observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 2, "Divergent HTTP observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Divergent inputs missing")?;
        for (name, raw) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
            fs::write(
                path,
                STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
            )?;
        }
        runtime.block_on(async {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                router(root.clone(), "fixture-api", "fixture-host")?.oneshot(
                    Request::post("/api/profiles/fixture00001/launch")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))?,
                ),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "Divergent HTTP status differs: case={index}"
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let actual: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(
                actual, case["body"],
                "Divergent HTTP response kind differs: case={index}"
            );
            for (name, raw) in files {
                assert_eq!(
                    fs::read(root.join(name))?,
                    STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
                    "Native divergent HTTP changed stored bytes"
                );
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
    }
    temporary.cleanup()?;
    Ok(())
}
