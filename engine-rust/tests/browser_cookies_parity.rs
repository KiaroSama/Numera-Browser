use numera_core::{
    browser::{adapter::Adapter, cookies},
    ownership::claims,
};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn cookie_write_matches_original_noop_clear_add_and_failure_order()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let source = common::TempDirectory::new("cookie-source-policy")?;
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/migration");
    let mut command = Command::new(&python);
    command.args(["-u", "-c", "import json,sys; from browser_cookie_cases import cases; r=json.loads(sys.stdin.readline()); print(json.dumps({'protocol':1,'id':r['id'],'ok':True,'result':{'cases':cases()}}),flush=True)"])
        .env("PYTHONPATH", format!("{}{}{}", module.display(), if cfg!(windows) { ";" } else { ":" }, module.parent().and_then(Path::parent).ok_or("Repository missing")?.join("engine").display()))
        .env("PYTHONUTF8", "1");
    let mut oracle = Adapter::spawn(
        source.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = oracle.metadata().clone();
    oracle.finish(Duration::from_secs(5))?;
    drop(oracle);
    source.cleanup()?;
    assert!(
        cookies::write(&[Value::Null], true, || panic!(
            "Invalid cookie activated SDK"
        ))
        .is_err()
    );
    let cases = document["cases"]
        .as_array()
        .ok_or("Original cases missing")?;
    assert_eq!(cases.len(), 6);
    for case in cases {
        let temporary = common::TempDirectory::new("cookie-write-policy")?;
        let root = temporary.path();
        let capture = root.join("seen.json");
        let jar = case["cookies"].as_array().ok_or("Cookie input missing")?;
        let clear = case["clear"].as_bool().ok_or("Clear input missing")?;
        let mut command = Command::new(&python);
        let script = "import json,os,pathlib,sys; seen=[]; failed=json.loads(os.environ['FIXTURE_FAILURE']); exec(\"for line in sys.stdin:\n r=json.loads(line)\n op=r['operation']\n if op=='hello': seen.append({'operation':'open'})\n elif op=='cookies-clear': seen.append({'operation':'clear'})\n elif op=='cookies-add': seen.append({'operation':'add','cookies':r['cookies']})\n elif op=='close': seen.append({'operation':'close'})\n else: raise ValueError('Unexpected operation')\n pathlib.Path(os.environ['FIXTURE_CAPTURE']).write_text(json.dumps(seen),encoding='utf-8')\n ok=not ((op=='cookies-clear' and failed=='clear') or (op=='cookies-add' and failed=='add'))\n print(json.dumps({'protocol':1,'id':r['id'],'ok':ok,'result':{'closing':True} if op=='close' else {'cleared':True} if op=='cookies-clear' else {'added':True} if op=='cookies-add' else {}}),flush=True)\n if op=='close': break\")";
        command
            .args(["-u", "-c", script])
            .env("FIXTURE_CAPTURE", &capture)
            .env("FIXTURE_FAILURE", serde_json::to_string(&case["failure"])?);
        let result = cookies::write(jar, clear, || {
            assert!(!jar.is_empty() || clear, "No-op activated a browser");
            Adapter::spawn(
                root,
                "a",
                "fixture-host",
                &mut command,
                Duration::from_secs(5),
            )
        });
        if case["outcome"]["failed"] == true {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(
                result?,
                case["outcome"]["count"].as_u64().ok_or("Count missing")? as usize
            );
        }
        let seen: Value = if capture.exists() {
            serde_json::from_slice(&std::fs::read(capture)?)?
        } else {
            Value::Array(Vec::new())
        };
        assert_eq!(seen, case["expected"], "{}", case["name"]);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        temporary.cleanup()?;
    }
    Ok(())
}
