use numera_core::{
    browser::adapter::Adapter,
    models::Profile,
    ownership::{claims, ready::StartupFailed, registry::Registry},
};
use serde_json::Map;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
mod common;

fn adapter(python: &std::ffi::OsStr, logs: &Path) -> Command {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Repository exists")
        .join("scripts/browser-adapter/adapter.py");
    let mut command = Command::new(python);
    command
        .arg("-u")
        .arg(script)
        .env("NUMERA_ADAPTER_LOG_DIR", logs)
        .env("PYTHONUTF8", "1");
    command
}
#[test]
fn actual_sdk_adapter_stays_inert_before_open_and_cleans_protocol_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    for invalid in [false, true] {
        let temporary = common::TempDirectory::new("browser-adapter-protocol")?;
        let root = temporary.path();
        let mut command = adapter(&python, &root.join("logs"));
        let mut sdk = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            sdk.metadata()["backends"]
                .as_object()
                .ok_or("Backend metadata missing")?
                .len(),
            4
        );
        assert!(
            !root.join("user-data").exists(),
            "Hello performed browser work"
        );
        if invalid {
            let failure = sdk.call("unsupported", Map::new(), Duration::from_secs(5));
            assert!(failure.is_err());
        } else {
            assert_eq!(
                sdk.call("close", Map::new(), Duration::from_secs(5))?["closing"],
                true
            );
        }
        sdk.finish(Duration::from_secs(5))?;
        drop(sdk);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let logs = std::fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(logs.len(), 1);
        let log = std::fs::read_to_string(logs[0].path())?;
        assert!(log.contains("[INFO] [numera.browser-adapter] Starting"));
        assert!(log.contains("Finished exit="));
        temporary.cleanup()?;
    }
    Ok(())
}
#[test]
fn fixed_adapter_observes_first_page_close_without_inventing_a_new_page()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("adapter-first-page")?;
    let root = temporary.path();
    let script = root.join("sdk_boundary.py");
    std::fs::write(
        &script,
        r#"
import importlib.util,os,pathlib,sys
spec=importlib.util.spec_from_file_location('fixed_adapter',os.environ['FIXED_ADAPTER'])
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class Page:
    def __init__(self): self.close_callback=None
    def on(self,event,callback):
        if event=='close': self.close_callback=callback
    def wait_for_timeout(self,timeout):
        if self.close_callback is None: raise AssertionError('First-page close event missing')
        self.close_callback()
        raise RuntimeError('SDK reports page already closed')
class Context:
    def __init__(self): self.pages=[Page()]
    def on(self,event,callback): pass
    def close(self): pathlib.Path(os.environ['FIXTURE_CLOSED']).write_text('closed',encoding='utf-8')
    def new_page(self): raise AssertionError('Closed page was replaced')
module.available=lambda:{'playwright':{'installed':True}}
module.open_context=lambda backend,options:(None,Context(),None)
raise SystemExit(module.main())
"#,
    )?;
    let fixed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/browser-adapter/adapter.py");
    let closed = root.join("closed");
    let mut command = Command::new(python);
    command
        .arg("-u")
        .arg(script)
        .env("FIXED_ADAPTER", fixed)
        .env("FIXTURE_CLOSED", &closed)
        .env("NUMERA_ADAPTER_FIRST_PAGE", "1")
        .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
        .env("PYTHONUTF8", "1");
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        sdk.open(
            "playwright",
            Map::from_iter([(
                "user_data_dir".into(),
                serde_json::json!(root.join("profile")),
            )]),
            Vec::new(),
            Duration::from_secs(5),
        )?;
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(code) = sdk.settled_exit()? {
                assert_eq!(code, 0, "First-page close became an SDK failure");
                break;
            }
            if Instant::now() >= end {
                return Err("First-page close did not finish the adapter".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(&closed)?, "closed");
        Ok(())
    })();
    let cleanup = sdk.finish(Duration::from_secs(5));
    cleanup?;
    result?;
    drop(sdk);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    temporary.cleanup()?;
    Ok(())
}
#[test]
fn registry_retains_sdk_transport_and_closes_only_its_admitted_session()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("browser-registry-protocol")?;
    let root = temporary.path();
    std::fs::create_dir_all(root.join("profiles"))?;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let profile = Profile::from_record(&serde_json::json!({
        "id":"a", "name":"fixture", "fingerprint":fixture["cases"][0]["input"]
    }))?;
    std::fs::write(root.join("profiles/a.json"), serde_json::to_vec(&profile)?)?;
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let foreign = Registry::new(root.into(), "fixture-host".into(), None)?;
    let failure = |error: std::io::Error| StartupFailed {
        kind: "failed".into(),
        message: error.to_string(),
    };
    assert!(registry.open_browser("a", "fixture", |session, current| {
        assert_eq!(current.id, "a");
        let mut command = adapter(&python, &root.join("logs"));
        Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(failure)
    })?);
    assert!(registry.owns("a")?);
    assert!(registry.running("a")?);
    assert!(!registry.open_browser("a", "fixture", |_, _| panic!(
        "Duplicate activated another SDK"
    ))?);
    assert!(!foreign.stop("a", Duration::from_secs(5))?);
    assert!(registry.running("a")?);
    assert!(registry.stop("a", Duration::from_secs(5))?);
    assert!(!registry.owns("a")?);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(registry.shutdown(Duration::from_secs(5)).is_empty());
    assert!(
        registry
            .open_browser("a", "fixture", |_, _| panic!("Shutdown activated SDK"))
            .is_err()
    );
    drop(registry);
    drop(foreign);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn acknowledged_close_waits_for_normal_exit_before_releasing_the_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("browser-close-exit")?;
    let root = temporary.path();
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let release = root.join("release");
    let completed = root.join("completed");
    let script = "import json,os,pathlib,sys,time; hello=json.loads(sys.stdin.readline()); print(json.dumps({'protocol':1,'id':hello['id'],'ok':True,'result':{}}),flush=True); close=json.loads(sys.stdin.readline()); print(json.dumps({'protocol':1,'id':close['id'],'ok':True,'result':{'closing':True}}),flush=True); ack=pathlib.Path(os.environ['FIXTURE_ACK']); ack.write_text('ack',encoding='utf-8'); release=pathlib.Path(os.environ['FIXTURE_RELEASE']); deadline=time.monotonic()+5; exec('while not release.exists() and time.monotonic()<deadline:\\n time.sleep(0.01)'); pathlib.Path(os.environ['FIXTURE_COMPLETED']).write_text('finished',encoding='utf-8')";
    let ack = root.join("ack");
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .env("FIXTURE_ACK", &ack)
        .env("FIXTURE_RELEASE", &release)
        .env("FIXTURE_COMPLETED", &completed);
    let adapter = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    registry.retain_browser("a", adapter)?;
    let stopping = std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let stop = scope.spawn(|| registry.stop("a", Duration::from_secs(5)));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ack.exists() {
            if std::time::Instant::now() >= deadline {
                return Err("Close acknowledgement missing".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::write(&release, b"release")?;
        assert!(stop.join().map_err(|_| "Stop thread failed")??);
        Ok(())
    });
    stopping?;
    assert_eq!(std::fs::read_to_string(completed)?, "finished");
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(!registry.owns("a")?);
    drop(registry);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fixed_adapter_maps_packaged_camoufox_enum_and_geoip_only_at_sdk_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("adapter-package-enum")?;
    let root = temporary.path();
    let script = root.join("sdk_boundary.py");
    let fixed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/browser-adapter/adapter.py");
    std::fs::write(
        &script,
        r#"
import importlib.util,json,os,pathlib,sys,types
from enum import Enum
spec=importlib.util.spec_from_file_location('fixed_adapter',os.environ['FIXED_ADAPTER'])
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class DefaultAddons(Enum): UBO='fixture'
addons=types.ModuleType('camoufox.addons');addons.DefaultAddons=DefaultAddons
geolocation=types.ModuleType('camoufox.geolocation')
package=types.ModuleType('camoufox');package.geolocation=geolocation
class Manager:
    def __init__(self,**options):
        assert options['exclude_addons']==[DefaultAddons.UBO]
        assert options['ff_version']=='152'
        assert str(geolocation.GEOIP_DIR)==os.environ['NUMERA_ADAPTER_GEOIP_DIR']
        assert geolocation.MMDB_DIR==geolocation.GEOIP_DIR/'mmdb'
        assert geolocation.GEOIP_CONFIG==geolocation.GEOIP_DIR/'config.yml'
        pathlib.Path(os.environ['FIXTURE_OBSERVED']).write_text('mapped',encoding='utf-8')
    def __enter__(self): return Context()
    def __exit__(self,*args): pass
class Page:
    def wait_for_timeout(self,timeout): pass
class Context:
    pages=[Page()]
    def on(self,event,callback): pass
    def close(self): pass
api=types.ModuleType('camoufox.sync_api');api.Camoufox=Manager
sys.modules.update({'camoufox':package,'camoufox.addons':addons,'camoufox.geolocation':geolocation,'camoufox.sync_api':api})
module.available=lambda:{'camoufox':{'installed':True}}
sys.exit(module.main())
"#,
    )?;
    let observed = root.join("observed");
    let mut command = Command::new(python);
    command
        .args(["-u", script.to_str().ok_or("Script path nontext")?])
        .env("FIXED_ADAPTER", fixed)
        .env("FIXTURE_OBSERVED", &observed)
        .env("NUMERA_ADAPTER_GEOIP_DIR", root.join("geoip"))
        .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
        .env("PYTHONUTF8", "1");
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let options = serde_json::json!({"user_data_dir":root.join("user-data/a"),"exclude_addons":["UBO"],"ff_version":"152"});
        sdk.open(
            "camoufox",
            options.as_object().cloned().ok_or("Options missing")?,
            Vec::new(),
            Duration::from_secs(5),
        )?;
        assert_eq!(std::fs::read_to_string(observed)?, "mapped");
        assert_eq!(
            sdk.call("close", Map::new(), Duration::from_secs(5))?["closing"],
            true
        );
        Ok(())
    })();
    sdk.finish(Duration::from_secs(5))?;
    result?;
    drop(sdk);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fixed_adapter_probe_calls_preserve_sdk_args_and_refuse_invalid_deadlines()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("adapter-probes")?;
    let root = temporary.path();
    let script = root.join("sdk_boundary.py");
    std::fs::write(
        &script,
        r#"
import importlib.util,json,os,pathlib,sys
spec=importlib.util.spec_from_file_location('fixed_adapter',os.environ['FIXED_ADAPTER'])
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
seen=[]
def record(value):
    seen.append(value)
    pathlib.Path(os.environ['FIXTURE_OBSERVED']).write_text(json.dumps(seen),encoding='utf-8')
class Mouse:
    def wheel(self,x,y):
        record({'operation':'wheel','x':x,'y':y})
        if y==-1: raise RuntimeError('Sensitive SDK detail must not escape')
    def move(self,x,y,steps): record({'operation':'move','x':x,'y':y,'steps':steps})
class Page:
    mouse=Mouse()
    def wait_for_timeout(self,timeout): pass
    def route(self,pattern,handler):
        record({'operation':'route','pattern':pattern})
        class Route:
            def fulfill(self,**kwargs): record({'operation':'fulfill',**kwargs})
        handler(Route())
    def goto(self,url,**kwargs):
        record({'operation':'goto','url':url,**kwargs})
        if url=='https://failure.fixture.test/': raise RuntimeError('Sensitive SDK detail must not escape')
    def evaluate(self,expression,argument=None):
        record({'operation':'evaluate','expression':expression,'argument':argument})
        if expression=='fail': raise RuntimeError('Sensitive SDK detail must not escape')
        if expression=='nonfinite': return float('nan')
        return {'argument':argument}
class Context:
    def __init__(self): self.pages=[Page()]
    def on(self,event,callback): pass
    def new_page(self):
        record({'operation':'new_page'})
        page=Page();self.pages.append(page);return page
    def close(self): record({'operation':'close'})
module.available=lambda:{'playwright':{'installed':True}}
module.open_context=lambda backend,options:(None,Context(),None)
sys.exit(module.main())
"#,
    )?;
    let fixed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/browser-adapter/adapter.py");
    for (index, invalid) in [
        serde_json::json!(0),
        serde_json::json!(true),
        serde_json::json!(45001),
        serde_json::json!("20"),
        serde_json::Value::Null,
    ]
    .into_iter()
    .enumerate()
    {
        let observed = root.join(format!("observed-{index}.json"));
        let mut command = Command::new(&python);
        command
            .arg("-u")
            .arg(&script)
            .env("FIXED_ADAPTER", &fixed)
            .env("FIXTURE_OBSERVED", &observed)
            .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
            .env("PYTHONUTF8", "1");
        let mut sdk = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            sdk.open(
                "playwright",
                Map::from_iter([(
                    "user_data_dir".into(),
                    serde_json::json!(root.join("profile")),
                )]),
                Vec::new(),
                Duration::from_secs(5),
            )?;
            if index == 0 {
                assert_eq!(sdk.call("route", serde_json::json!({"pattern":"**/*","status":200,"content_type":"text/html","body":"<title>fixture</title>"}).as_object().cloned().ok_or("Route missing")?,Duration::from_secs(5))?["routed"],true);
                for (url, timeout, expected) in [
                    ("https://ok.fixture.test/", 20000, true),
                    ("https://failure.fixture.test/", 30000, false),
                    ("https://warmup.fixture.test/", 45000, true),
                ] {
                    let result = sdk.call(
                        "navigate",
                        serde_json::json!({"url":url,"timeout_ms":timeout,"tolerate_failure":true})
                            .as_object()
                            .cloned()
                            .ok_or("Navigation missing")?,
                        Duration::from_secs(5),
                    )?;
                    assert_eq!(result["navigated"], expected);
                }
                for (fields, expected) in [
                    (serde_json::json!({"action":"wheel","x":0,"y":200}), true),
                    (
                        serde_json::json!({"action":"move","x":50,"y":80,"steps":5}),
                        true,
                    ),
                    (serde_json::json!({"action":"wheel","x":0,"y":-1}), false),
                ] {
                    assert_eq!(
                        sdk.call(
                            "mouse",
                            fields.as_object().cloned().ok_or("Mouse fields missing")?,
                            Duration::from_secs(5)
                        )?["performed"],
                        expected
                    );
                }
                for (expression, expected) in [("success", true), ("fail", false)] {
                    let result = sdk.call("evaluate", serde_json::json!({"expression":expression,"argument":{"n":8},"tolerate_failure":true}).as_object().cloned().ok_or("Evaluation missing")?, Duration::from_secs(5))?;
                    assert_eq!(result["evaluated"], expected);
                    assert_eq!(
                        result["value"],
                        if expected {
                            serde_json::json!({"argument":{"n":8}})
                        } else {
                            serde_json::Value::Null
                        }
                    );
                }
            }
            assert!(sdk.call("navigate", serde_json::json!({"url":"https://ok.fixture.test/","new_page":true,"timeout_ms":invalid,"tolerate_failure":true}).as_object().cloned().ok_or("Invalid navigation missing")?, Duration::from_secs(5)).is_err());
            Ok(())
        })();
        sdk.finish(Duration::from_secs(5))?;
        result?;
        let seen: serde_json::Value = serde_json::from_slice(&std::fs::read(observed)?)?;
        let seen = seen.as_array().ok_or("SDK observations missing")?;
        assert!(!seen.iter().any(|row| row["operation"] == "new_page"));
        assert_eq!(
            seen.last().ok_or("SDK close missing")?["operation"],
            "close"
        );
        if index == 0 {
            assert_eq!(seen[0]["pattern"], "**/*");
            assert_eq!(
                seen[1],
                serde_json::json!({"operation":"fulfill","status":200,"content_type":"text/html","body":"<title>fixture</title>"})
            );
            assert_eq!(seen[2]["timeout"], 20000);
            assert_eq!(seen[2]["wait_until"], "domcontentloaded");
            assert_eq!(seen[4]["timeout"], 45000);
            assert_eq!(
                seen[5],
                serde_json::json!({"operation":"wheel","x":0,"y":200})
            );
            assert_eq!(
                seen[6],
                serde_json::json!({"operation":"move","x":50,"y":80,"steps":5})
            );
            assert_eq!(seen.len(), 11);
        } else {
            assert_eq!(seen.len(), 1, "Invalid deadline reached the SDK");
        }
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    }
    let observed = root.join("nonfinite.json");
    let mut command = Command::new(python);
    command
        .arg("-u")
        .arg(script)
        .env("FIXED_ADAPTER", fixed)
        .env("FIXTURE_OBSERVED", &observed)
        .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
        .env("PYTHONUTF8", "1");
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        sdk.open(
            "playwright",
            Map::from_iter([(
                "user_data_dir".into(),
                serde_json::json!(root.join("profile")),
            )]),
            Vec::new(),
            Duration::from_secs(5),
        )?;
        assert!(
            sdk.call(
                "evaluate",
                serde_json::json!({"expression":"nonfinite","tolerate_failure":true})
                    .as_object()
                    .cloned()
                    .ok_or("Evaluation missing")?,
                Duration::from_secs(5)
            )
            .is_err(),
            "Serialization failure was swallowed as an SDK failure"
        );
        Ok(())
    })();
    sdk.finish(Duration::from_secs(5))?;
    result?;
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    for entry in std::fs::read_dir(root.join("logs"))? {
        assert!(!std::fs::read_to_string(entry?.path())?.contains("Sensitive SDK detail"));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_transport_refuses_wrong_envelope_partial_frame_and_unresponsive_writer()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    for script in [
        "import sys,json; sys.stdin.readline(); print(json.dumps({'protocol':2,'id':1,'ok':True,'result':{}}),flush=True)",
        "import sys; sys.stdin.readline(); sys.stdout.write('{'); sys.stdout.flush()",
        "import threading; threading.Event().wait()",
    ] {
        let temporary = common::TempDirectory::new("browser-adapter-refusal")?;
        let mut command = Command::new(&python);
        command.args(["-u", "-c", script]);
        let result = Adapter::spawn(
            temporary.path(),
            "a",
            "fixture-host",
            &mut command,
            Duration::from_millis(250),
        );
        assert!(result.is_err());
        assert!(matches!(
            claims::read(temporary.path(), "a"),
            claims::Claim::Absent
        ));
        temporary.cleanup()?;
    }
    Ok(())
}
