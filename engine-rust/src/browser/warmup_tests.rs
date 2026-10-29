use super::{adapter::Adapter, runtime::Runtime, warmup, warmup_policy};
use crate::common;
use crate::{
    cli::warmup_commands::{Options, Warmup},
    ownership::registry::Registry,
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

struct Clock(f64, std::path::PathBuf);
impl warmup::Control for Clock {
    fn elapsed(&self) -> f64 {
        self.0
            + std::fs::read_to_string(&self.1)
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .map_or(0.0, |value| f64::from(value) * 0.01)
    }
    fn pause(&mut self, seconds: f64) -> std::io::Result<()> {
        self.0 += seconds;
        Ok(())
    }
    fn random(&mut self) -> f64 {
        0.0
    }
    fn below(&mut self, _: usize) -> usize {
        0
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}
#[test]
fn original_warmup_validation_presets_and_native_control_match_without_real_waits()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("warmup-control")?;
    let root = temporary.path();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(&python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from warmup_cases import cases,run_cases,schedule_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'policy':cases(),'runs':run_cases(),'schedules':schedule_cases(os.environ['FIXTURE_ROOT'])}}),flush=True)
"#]).env("FIXTURE_ROOT",root.join("source-store")).env("PYTHONUTF8","1").env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        root,
        "source",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call("observe", Default::default(), Duration::from_secs(20))?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    for case in document["policy"]["validation"]
        .as_array()
        .ok_or("Validation cases missing")?
    {
        match warmup_policy::validate(
            &case["minutes"],
            Some(&case["preset"]),
            Some(&case["sites"]),
        ) {
            Ok(value) => {
                assert!(case["error"].is_null());
                assert_eq!(json!(value), case["value"]);
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Source error missing")?
            ),
        }
    }
    for (preset, urls) in document["policy"]["presets"]
        .as_object()
        .ok_or("Presets missing")?
    {
        assert_eq!(json!(warmup_policy::sites(Some(preset))), *urls);
    }
    assert_eq!(
        json!(warmup_policy::sites(Some("missing"))),
        document["policy"]["fallback"]
    );
    assert_eq!(warmup::HOST, document["policy"]["hostScript"]);
    assert_eq!(warmup::LINKS, document["policy"]["linksScript"]);
    let runs = document["runs"]
        .as_array()
        .ok_or("Warmup run cases missing")?;
    assert_eq!(runs.len(), 2);
    let script = root.join("sdk.py");
    std::fs::write(
        &script,
        r#"
import json,os,pathlib,sys
seen=[]
for line in sys.stdin:
    request=json.loads(line);op=request['operation']
    if op=='navigate':
        seen.append({'operation':'navigate','url':request['url'],'timeout':request['timeout_ms']})
        failed=os.environ.get('FIXTURE_FAILED','0')=='1'
        if failed: pathlib.Path(__file__).with_suffix('.ticks').write_text(str(len(seen)),encoding='utf-8')
        result={'navigated':not failed}
    elif op=='mouse':
        seen.append({'operation':request['action'],'x':request['x'],'y':request['y']})
        result={'performed':True}
    elif op=='open': result={'opened':True,'backend':request['backend']}
    elif op=='hello': result={'backends':{'playwright':{'installed':True}}}
    else: result={'closing':True} if op=='close' else {}
    pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
    )?;
    for case in runs {
        let ticks = script.with_extension("ticks");
        if ticks.exists() {
            std::fs::remove_file(&ticks)?;
        }
        let mut command = Command::new(&python);
        command.arg("-u").arg(&script).env("PYTHONUTF8", "1").env(
            "FIXTURE_FAILED",
            if case["failed"] == true { "1" } else { "0" },
        );
        let mut sdk = Adapter::spawn(
            root,
            "native",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let mut clock = Clock(0.0, ticks);
        let result = warmup::run(
            &mut sdk,
            0.001,
            vec!["https://fixture.test/".into()],
            "custom",
            0.0,
            &mut clock,
            |_| Ok(()),
        );
        sdk.finish(Duration::from_secs(5))?;
        drop(sdk);
        assert_eq!(result?, case["summary"]);
        let operations: Value =
            serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        assert_eq!(operations, case["operations"]);
        assert_eq!(case["closed"], json!([true]));
        assert!(matches!(
            crate::ownership::claims::read(root, "native"),
            crate::ownership::claims::Claim::Absent
        ));
    }
    let native = root.join("service");
    let repo = ProfileRepo::new(native.clone(), None);
    let fingerprint = crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(
        crate::models::Profile::from_record(
            &json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint,
        "startup_urls":["https://must-not-navigate.fixture.test/"]}),
        )?,
        1000.0,
    )?;
    let sessions = Registry::new(native.clone(), "fixture-host".into(), None)?;
    let browser = Runtime {
        python: python.into(),
        adapter: script.clone(),
        cwd: root.into(),
        home: root.into(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
        deployment: None,
    };
    let service = Warmup {
        repo: &repo,
        sessions: &sessions,
        runtime: &browser,
    };
    let mut output = crate::cli::output::Output::new(Vec::new(), Vec::new(), false, false);
    let mut clock = Clock(0.0, root.join("unused-ticks"));
    let minutes = json!(0.001);
    let urls = json!(["https://fixture.test/"]);
    let result = service.run_output_controlled(
        Options {
            reference: "Fixture",
            minutes: &minutes,
            preset: None,
            sites: Some(&urls),
            engine: None,
            headless: true,
            time: 1000.0,
        },
        &mut clock,
        &mut output,
    )?;
    assert_eq!(result.exit, 0);
    let mut expected = document["runs"][0]["summary"].clone();
    expected["ok"] = true.into();
    expected["id"] = "fixture".into();
    expected["name"] = "Fixture".into();
    assert_eq!(result.payload, expected);
    let (stdout, stderr) = output.into_parts();
    assert_eq!(
        String::from_utf8(stdout)?,
        document["runs"][0]["human"][0]["stdout"]
            .as_str()
            .ok_or("Source warmup output missing")?
    );
    assert!(stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(script.with_extension("seen"))?)?,
        document["runs"][0]["operations"]
    );
    assert!(!sessions.owns("fixture")?);
    assert!(matches!(
        crate::ownership::claims::read(&native, "fixture"),
        crate::ownership::claims::Claim::Absent
    ));
    assert_eq!(
        repo.get("fixture")?
            .ok_or("Stored profile missing")?
            .startup_urls,
        vec!["https://must-not-navigate.fixture.test/"]
    );
    let schedules = crate::storage::schedules::ScheduleStore::new(native.clone());
    for case in document["schedules"]
        .as_array()
        .ok_or("Schedule source cases missing")?
    {
        std::fs::write(
            native.join("schedules.json"),
            serde_json::to_vec(&case["rows"])?,
        )?;
        let mut calls = Vec::new();
        let mut output = crate::cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let outcome = service.run_due_with_output(1000.0,||Ok(1000.0),|options| {
            calls.push(json!({"id":options.reference,"minutes":options.minutes,"preset":options.preset,"headless":options.headless}));
            if case["mode"] == "failure" {return Err(crate::storage::repo::RepoError::Message("Controlled warmup failure".into()));}
            Ok(())
        }, &mut output);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["human"][0]["stdout"]
                .as_str()
                .ok_or("Run-due source output missing")?
        );
        assert!(stderr.is_empty());
        match outcome {
            Ok(outcome) => {
                assert!(case["error"].is_null());
                assert_eq!(outcome.payload, case["payload"]);
                assert_eq!(
                    i64::from(outcome.exit),
                    case["exit"].as_i64().ok_or("Schedule exit missing")?
                );
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Schedule error missing")?
            ),
        }
        assert_eq!(json!(calls), case["calls"]);
        assert_eq!(json!(schedules.list()?), case["stored"]);
    }
    assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
