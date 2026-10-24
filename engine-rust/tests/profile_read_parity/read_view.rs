use super::common;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    http::profile_routes::router, models::Profile, ownership::capture, read_view,
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};
use tower::ServiceExt;

#[test]
fn legacy_read_views_match_original_core_http_and_name_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("legacy-read-views")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import write_contract
from numera.server.routes_profiles import readable as dashboard_readable
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from numera.models import Profile
from run_logging import run_log
values=[None,False,True,'', 'plain α 日本語',17,-17,0.0,-0.0,1.0,1.23456789,
        1e-5,1e-4,999999.5,1000000.0,1e100,5e-324,
        9007199254740993,[],['α',1,None,True,'z'],{}, {'nested':[True,None,1.25]}]
views=[]
with run_log('legacy-read-views-oracle',parent/'logs') as log:
    for value in values:
        record={'name':value,'notes':value,'tags':value,'future':{'retained':[value]}}
        before=copy.deepcopy(record)
        views.append({'input':record,'default':write_contract.readable(record),
                      'dashboard':dashboard_readable(record),
                      'custom':write_contract.readable(record,text_fields=('notes',),list_fields=('future',)),
                      'name':write_contract.readable_value('name',value),
                      'tags':write_contract.readable_value('tags',value),
                      'names':write_contract.readable_names([('a',value),('b','unchanged')])})
        assert record==before,'Original read view changed input'
    for record in ({},{'notes':None,'future':{'x':[1,2]}}):
        views.append({'input':record,'default':write_contract.readable(record),
                      'dashboard':dashboard_readable(record),
                      'custom':write_contract.readable(record,text_fields=('notes',),list_fields=('future',))})
    base=json.loads((repository/'engine-rust/tests/fixtures/repo-write.json').read_text(encoding='utf-8'))['cases'][0]['initial']['profiles']
    names=[]
    for value in [1.0,-1.0,1.23456789,1e-5,999999.5,1e100,17,True]:
        for directory in ('profiles','dashboard'):
            root=parent/str(len(names));store=ProfileStore(root);repo=ProfileRepo(store)
            old={'id':'aaaaaaaaaaaa','name':value}
            raw=json.dumps(old,ensure_ascii=True).encode('utf-8')
            path=root/directory/'aaaaaaaaaaaa.json';path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
            candidate={**base,'id':'bbbbbbbbbbbb','name':write_contract.readable_value('name',value)}
            with patch('numera.repo.time.time',return_value=1700000000.0):
                try: repo.save_many([Profile.from_dict(candidate)]);error=None
                except ValueError as exc:error=str(exc)
            assert error is not None and path.read_bytes()==raw,'Original legacy name was not protected'
            names.append({'record':old,'directory':directory,'candidate':candidate,'error':error})
    log.info('Observed read views=%s name refusals=%s',len(views),len(names))
    print(json.dumps({'views':views,'names':names},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source")).env("PYTHONUTF8","1");
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 524288,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original legacy read-view observer failed"
    );
    let observed: Value = serde_json::from_slice(&original.stdout)?;
    let views = observed["views"].as_array().ok_or("Read views missing")?;
    assert_eq!(views.len(), 24, "Legacy read observations missing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for (index, case) in views.iter().enumerate() {
        let record = case["input"].clone();
        assert_eq!(
            read_view::readable(&record),
            case["default"],
            "Core legacy read view differs: {index}"
        );
        assert_eq!(
            read_view::readable_with(&record, &["notes"], &["future"]),
            case["custom"],
            "Custom legacy read view differs: {index}"
        );
        if let Some(value) = record.get("name") {
            assert_eq!(
                read_view::readable_value("name", value),
                case["name"],
                "Legacy name text differs: {index}"
            );
            assert_eq!(
                read_view::readable_value("tags", value),
                case["tags"],
                "Legacy list view differs: {index}"
            );
            let pairs = vec![
                ("a".to_owned(), value.clone()),
                ("b".to_owned(), json!("unchanged")),
            ];
            assert_eq!(
                serde_json::to_value(read_view::readable_names(&pairs))?,
                case["names"],
                "Legacy name pairs differ: {index}"
            );
        }
        assert_eq!(record, case["input"], "Read helper changed its input");
        let root = temporary.path().join(format!("http-{index}"));
        fs::create_dir_all(root.join("dashboard"))?;
        let mut stored = record.clone();
        stored["id"] = json!("aaaaaaaaaaaa");
        let bytes = serde_json::to_vec(&stored)?;
        let path = root.join("dashboard/aaaaaaaaaaaa.json");
        fs::write(&path, &bytes)?;
        let app = router(root, "fixture-api", "fixture-host")?;
        let response = runtime.block_on(async {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::get("/api/profiles")
                        .header("authorization", "Bearer fixture-api")
                        .body(Body::empty())?,
                ),
            )
            .await??;
            let status = response.status().as_u16();
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 65536),
            )
            .await??;
            Ok::<_, Box<dyn std::error::Error>>((status, serde_json::from_slice::<Value>(&bytes)?))
        })?;
        assert_eq!(response.0, 200, "Legacy HTTP read refused: {index}");
        let mut expected = case["dashboard"].clone();
        expected["id"] = json!("aaaaaaaaaaaa");
        expected["status"] = json!("stopped");
        expected["lastActive"] = json!("never");
        assert_eq!(
            response.1,
            json!([expected]),
            "Legacy HTTP read view differs: {index}"
        );
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "HTTP read rewrote legacy bytes: {index}"
        );
    }
    let names = observed["names"]
        .as_array()
        .ok_or("Name refusal observations missing")?;
    assert_eq!(names.len(), 16, "Name refusal observations missing");
    for (index, case) in names.iter().enumerate() {
        let root = temporary.path().join(format!("names-{index}"));
        let directory = case["directory"].as_str().ok_or("Directory missing")?;
        assert!(matches!(directory, "profiles" | "dashboard"));
        fs::create_dir_all(root.join(directory))?;
        let path = root.join(directory).join("aaaaaaaaaaaa.json");
        let bytes = serde_json::to_vec(&case["record"])?;
        fs::write(&path, &bytes)?;
        let repo = ProfileRepo::new(root.clone(), None);
        let profile = Profile::from_record(&case["candidate"])?;
        let result = repo.save(profile, 1700000000.0);
        assert!(
            result.is_err(),
            "Legacy stored name admission differs: case={index}"
        );
        assert_eq!(
            result.err().ok_or("Name refusal missing")?.to_string(),
            case["error"].as_str().ok_or("Expected error missing")?,
            "Legacy name refusal differs: {index}"
        );
        assert_eq!(
            fs::read(path)?,
            bytes,
            "Name refusal changed legacy bytes: {index}"
        );
        assert!(!root.join("profiles/bbbbbbbbbbbb.json").exists());
        assert!(!root.join("dashboard/bbbbbbbbbbbb.json").exists());
    }
    temporary.cleanup()?;
    Ok(())
}
