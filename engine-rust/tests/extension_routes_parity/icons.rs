use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::{http::extension_routes::router, ownership::capture};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};
use tower::ServiceExt;

#[test]
fn extension_icon_http_matches_original_known_folder_and_file_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::common::TempDirectory::new("extension-icon-http")?;
    let root = temporary.path().canonicalize()?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import base64,json,os,sys
from pathlib import Path
from urllib.parse import quote,urlencode
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from fastapi.testclient import TestClient
from numera.server import create_app
from run_logging import run_log

with run_log('extension-icon-http-oracle',root/'logs') as log:
    with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
        app=create_app(data_dir=str(root),start_scheduler=False)
        installed=root/'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
        manual=root/'manual plus + 日本語'
        empty=root/'no-icon'; unsafe=root/'unsafe'; unknown=root/'unknown'
        files={}
        for folder,manifest in [
            (installed,{'name':'Fixture','icons':{'16':'small.png','128':'large.png'}}),
            (manual,{'name':'Manual','action':{'default_icon':'icon.svg'}}),
            (empty,{'name':'Empty'}),
            (unsafe,{'name':'Unsafe','icons':{'128':'../protected.png','16':'missing.png'}}),
            (unknown,{'name':'Unknown','icons':{'16':'icon.png'}}),
        ]:
            files[folder/'manifest.json']=json.dumps(manifest,ensure_ascii=True).encode('utf-8')
        files.update({installed/'small.png':b'small',installed/'large.png':b'\x89PNG\r\n\x1a\n\x00\xfffixture',
                      manual/'icon.svg':b'<svg xmlns="http://www.w3.org/2000/svg"/>',
                      unknown/'icon.png':b'unknown',root/'protected.png':b'protected-bytes'})
        config=root/'config.json'
        current=json.loads(config.read_text(encoding='utf-8')) if config.exists() else {}
        current['default_extensions']=[str(manual),str(empty),str(unsafe),'https://example.invalid/extension']
        files[config]=json.dumps(current,ensure_ascii=True).encode('utf-8')
        for path,raw in files.items():
            path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
        cases=[]
        def add(label,query,method='GET',authorized=True,headers=None):
            cases.append((label,'/api/extensions/icon'+query,method,authorized,headers or {}))
        def query(path):return '?'+urlencode({'path':str(path)})
        add('installed',query(installed))
        add('manual-unicode-plus',query(manual))
        add('percent-space','?path='+quote(str(manual),safe=''))
        add('dot-components',query(installed/'..'/installed.name))
        add('literal-dot-components',query(str(installed)+os.sep+'..'+os.sep+installed.name))
        add('empty-icon',query(empty))
        add('unsafe-icon',query(unsafe))
        add('unknown-existing',query(unknown))
        add('unknown-missing',query(root/'missing'))
        add('empty-path','?path=')
        add('missing-path','')
        add('other-query','?other=value')
        add('duplicate-known-last',query(unknown)+'&'+query(installed)[1:])
        add('duplicate-unknown-last',query(installed)+'&'+query(unknown)[1:])
        add('range',query(installed),headers={'range':'bytes=1-4'})
        add('unauthorized',query(installed),authorized=False)
        add('head',query(installed),method='HEAD')
        rows=[]
        with TestClient(app,raise_server_exceptions=False) as client:
            for label,uri,method,authorized,extra in cases:
                headers=dict(extra)
                if authorized:headers['authorization']='Bearer fixture-api'
                response=client.request(method,uri,headers=headers)
                body=(response.json() if method!='HEAD' and response.headers.get('content-type','').startswith('application/json')
                      else base64.b64encode(response.content).decode('ascii'))
                selected={name:response.headers[name] for name in ['content-type','content-length','content-range','accept-ranges']
                          if name in response.headers}
                assert all(path.read_bytes()==raw for path,raw in files.items()),'Original icon request changed fixture bytes'
                rows.append({'label':label,'uri':uri,'method':method,'authorized':authorized,'headers':extra,
                             'status':response.status_code,'body':body,'response_headers':selected})
        assert rows[0]['status']==200 and rows[1]['status']==200,'Original icon success seam missing'
        assert next(row for row in rows if row['label']=='missing-path')['status']==422 and rows[-1]['status']==405,'Original icon refusal seam differs'
        log.info('Observed extension icon HTTP cases=%s',len(rows))
        print(json.dumps({'cases':rows,'files':{str(path.relative_to(root)):base64.b64encode(raw).decode('ascii')
                                              for path,raw in files.items()}},ensure_ascii=True))
"#]).arg(repository).arg(&root).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original extension icon HTTP observer failed"
    );
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = document["cases"].as_array().ok_or("Icon cases missing")?;
    assert_eq!(cases.len(), 17, "Extension icon HTTP observations missing");
    let files = document["files"].as_object().ok_or("Icon inputs missing")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let app = router(root.clone(), "fixture-api");
        for case in cases {
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["uri"].as_str().ok_or("URI missing")?);
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api");
            }
            for (name, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request =
                    request.header(name.as_str(), value.as_str().ok_or("Header value missing")?);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "Extension icon HTTP status differs: {}",
                case["label"]
            );
            let headers = response.headers().clone();
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let body = if case["method"] != "HEAD"
                && headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with("application/json"))
            {
                serde_json::from_slice(&bytes)?
            } else {
                Value::String(STANDARD.encode(&bytes))
            };
            assert_eq!(
                body, case["body"],
                "Extension icon HTTP body differs: {}",
                case["label"]
            );
            let actual = [
                "content-type",
                "content-length",
                "content-range",
                "accept-ranges",
            ]
            .into_iter()
            .filter_map(|name| headers.get(name).map(|value| (name, value)))
            .map(|(name, value)| Ok((name.to_owned(), Value::String(value.to_str()?.to_owned()))))
            .collect::<Result<serde_json::Map<String, Value>, axum::http::header::ToStrError>>()?;
            assert_eq!(
                json!(actual),
                case["response_headers"],
                "Extension icon HTTP headers differ: {}",
                case["label"]
            );
            for (name, raw) in files {
                assert_eq!(
                    fs::read(root.join(name))?,
                    STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?,
                    "Native icon request changed fixture bytes"
                );
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}
