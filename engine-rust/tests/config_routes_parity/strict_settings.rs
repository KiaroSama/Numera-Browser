use super::{BTreeMap, Body, Duration, Request, ServiceExt, Value, common, fs, to_bytes};
use numera_core::{
    http::application,
    ownership::{capture, registry::Registry},
};
use serde_json::json;
use std::{path::Path, process::Command, sync::Arc};

#[test]
fn settings_http_preserves_original_text_refusals_and_short_circuits()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-http")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
os.environ['NUMERA_API_TOKEN']='fixture-api'
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from fastapi.testclient import TestClient
from numera.server import create_app
from run_logging import run_log
operations=[('GET','/api/config',None),('GET','/api/engines',None),
            ('GET','/api/extensions/installed',None),
            ('PUT','/api/config',{'unknown':True}),
            ('POST','/api/profiles/fixture/attach',None),
            ('POST','/api/profiles',{'unknown':True})]
# Invalid text is distinct from malformed JSON and nonobject fallbacks.
settings=[bytes([255]),bytes([0xc3,0x28]),bytes([0xe2,0x82]),b'{}',b'{broken',b'[]',None]
available={'camoufox':False,'cloak':False,'patchright':False,'playwright':True}
profile={'id':'fixture','name':'Fixture','engine':'camoufox'}
observed=[]
with run_log('strict-settings-http-oracle',parent/'logs') as log:
    for method,url,body in operations:
        for raw in settings:
            root=parent/str(len(observed));root.mkdir(parents=True)
            app=create_app(str(root),start_scheduler=False)
            path=root/'config.json'
            if raw is not None:path.write_bytes(raw)
            stored=root/'dashboard/fixture.json'
            stored.write_text(json.dumps(profile),encoding='utf-8')
            before=stored.read_bytes()
            with patch('numera.drivers.available',return_value=available), \
                    patch('numera.drivers.is_installed',side_effect=lambda name:available.get(name,False)), \
                    TestClient(app,raise_server_exceptions=False) as client:
                response=client.request(method,url,json=body,headers={'Authorization':'Bearer fixture-api'})
            try:result=response.json()
            except ValueError:result=response.text
            if isinstance(result,dict) and 'root' in result:
                assert result['root']==str(root.resolve());result['root']='fixture-root'
            assert (path.read_bytes() if path.exists() else None)==raw
            assert stored.read_bytes()==before
            observed.append({'method':method,'url':url,'body':body,
                'raw':list(raw) if raw is not None else None,'status':response.status_code,'result':result})
    log.info('Observed strict settings HTTP cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original strict settings HTTP observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 42);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        for (index, case) in cases.iter().enumerate() {
            let root = temporary.path().join(format!("native-{index}"));
            fs::create_dir(&root)?;
            fs::create_dir(root.join("dashboard"))?;
            let profile =
                serde_json::to_vec(&json!({"id":"fixture","name":"Fixture","engine":"camoufox"}))?;
            let stored = root.join("dashboard/fixture.json");
            fs::write(&stored, &profile)?;
            let path = root.join("config.json");
            let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
            if let Some(raw) = &raw {
                fs::write(&path, raw)?;
            }
            let sessions = Arc::new(Registry::new(root.clone(), "fixture-host".into(), None)?);
            let installed = BTreeMap::from([
                ("camoufox".into(), false),
                ("cloak".into(), false),
                ("patchright".into(), false),
                ("playwright".into(), true),
            ]);
            let app = application::router_with_sessions(
                root.clone(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                application::SessionApplication {
                    installed,
                    sessions: sessions.clone(),
                },
            )
            .await?;
            let request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["url"].as_str().ok_or("URL missing")?)
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(if case["body"].is_null() {
                    Body::empty()
                } else {
                    Body::from(serde_json::to_vec(&case["body"])?)
                })?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.oneshot(request)).await??;
            assert_eq!(
                json!(response.status().as_u16()),
                case["status"],
                "Strict settings HTTP status differs: case={index} {}",
                case["url"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 16384),
            )
            .await??;
            let mut value = serde_json::from_slice::<Value>(&bytes)
                .unwrap_or(Value::String(String::from_utf8(bytes.to_vec())?));
            if value.get("root").is_some() {
                assert_eq!(value["root"], json!(root));
                value["root"] = "fixture-root".into();
            }
            assert_eq!(
                value, case["result"],
                "HTTP body differs: case={index} {}",
                case["url"]
            );
            assert_eq!(fs::read(path).ok(), raw, "HTTP changed settings bytes");
            assert_eq!(fs::read(stored)?, profile, "HTTP changed profile bytes");
            assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}
