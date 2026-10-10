use numera_core::{
    browser::{adapter::Adapter, startup},
    ownership::claims,
};
use serde_json::Value;
use std::{process::Command, time::Duration};
mod common;

#[test]
fn startup_navigation_preserves_original_url_order_tabs_and_per_url_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-startup.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Startup cases missing")?;
    assert_eq!(cases.len(), 4);
    for case in cases {
        let temporary = common::TempDirectory::new("startup-navigation")?;
        let root = temporary.path();
        let capture = root.join("seen.json");
        let script = "import json,os,pathlib,sys; seen=[]; tab=0; failed=json.loads(os.environ['FIXTURE_FAILURES']); exec(\"for line in sys.stdin:\\n r=json.loads(line)\\n if r['operation']=='navigate':\\n  assert type(r['tolerate_failure']) is bool and r['tolerate_failure']\\n  if r['new_page']: tab+=1\\n  index=len(seen)\\n  seen.append({'tab':tab,'url':r['url'],'wait_until':'domcontentloaded','timeout':30000})\\n  pathlib.Path(os.environ['FIXTURE_CAPTURE']).write_text(json.dumps(seen),encoding='utf-8')\\n  result={'navigated':index not in failed}\\n else: result={}\\n print(json.dumps({'protocol':1,'id':r['id'],'ok':True,'result':result}),flush=True)\")";
        std::fs::write(&capture, b"[]")?;
        let mut command = Command::new(&python);
        command
            .args(["-u", "-c", script])
            .env("FIXTURE_CAPTURE", &capture)
            .env("FIXTURE_FAILURES", serde_json::to_string(&case["failed"])?);
        let mut adapter = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let urls: Vec<String> = serde_json::from_value(case["urls"].clone())?;
        startup::navigate(&mut adapter, &urls)?;
        adapter.finish(Duration::from_secs(5))?;
        drop(adapter);
        let seen: Value = serde_json::from_slice(&std::fs::read(capture)?)?;
        assert_eq!(seen, case["expected"], "{}", case["name"]);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        temporary.cleanup()?;
    }
    Ok(())
}
