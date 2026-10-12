use numera_core::{
    browser::{adapter::Adapter, runtime::Runtime},
    cli::{self, application::Application},
    ownership::{claims, registry::Registry},
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;
#[path = "support/probe_output.rs"]
mod probe_output;

#[test]
fn cookie_cli_services_preserve_source_refusals_output_and_owned_sdk_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("cli-cookie-parity")?;
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
from cli_cookie_cases import cases
from probe_cases import cases as probe_cases, alignment_cases
from cli_probe_output_cases import cases as human_probes
probes=probe_cases();alignment=alignment_cases(os.path.join(os.environ['FIXTURE_ROOT'],'alignment'))
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'probes':probes,'alignment':alignment,'humanProbes':human_probes(probes,alignment),'humanCookies':cases(os.path.join(os.environ['FIXTURE_ROOT'],'human-cookies'),human=True)}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",root.join("source"))
        .env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        root,
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-cookie-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    probe_output::compare(&document["humanProbes"])?;
    let native = root.join("native");
    let repo = ProfileRepo::open(native.clone(), None, None)?;
    let fingerprint =
        numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
            seed: 42,
            ..Default::default()
        })?;
    repo.save(
        numera_core::models::Profile::from_record(
            &json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint}),
        )?,
        1000.0,
    )?;
    let row = json!({"name":"fixture","value":"synthetic","domain":".fixture.test","path":"/","expires":-1.0,"httpOnly":false,"secure":false,"sameSite":"Lax"});
    std::fs::write(native.join("empty.json"), "[]")?;
    std::fs::write(native.join("rows.json"), serde_json::to_vec(&json!([row]))?)?;
    let adapter = native.join("sdk.py");
    std::fs::write(
        &adapter,
        r#"
import json,pathlib,sys
seen=[]
for line in sys.stdin:
    request=json.loads(line);op=request['operation'];seen.append(op)
    if op=='hello': result={'backends':{'playwright':{'installed':True}}}
    elif op=='open': result={'opened':True,'backend':request['backend']}
    elif op=='cookies-read': result={'cookies':[{'name':'fixture','value':'synthetic','domain':'.fixture.test'}]}
    elif op=='cookies-clear': result={'cleared':True}
    elif op=='cookies-add': result={'added':True}
    else: result={'closing':True}
    pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close':break
"#,
    )?;
    let sessions = Registry::new(native.clone(), "fixture-host".into(), None)?;
    let browser = Runtime {
        python: python.into(),
        adapter,
        cwd: native.clone(),
        home: native.clone(),
        logs: native.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
    };
    let installed = std::collections::BTreeMap::new();
    let services = Application {
        root: &native,
        home: &native,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: None,
        environment_token: false,
        sessions: &sessions,
        browser: Some(&browser),
    };
    let mut registry = cli::registry();
    registry.build();
    let cases = document["cases"]
        .as_array()
        .ok_or("Cookie CLI cases missing")?;
    assert_eq!(cases.len(), 7);
    assert_eq!(
        document["humanCookies"]
            .as_array()
            .ok_or("Cookie human cases missing")?
            .len(),
        7
    );
    for (index, case) in cases.iter().enumerate() {
        let original = case["argv"].as_array().ok_or("Source argv missing")?;
        let mut argv = original
            .iter()
            .map(|v| v.as_str().expect("Source argv text").to_owned())
            .collect::<Vec<_>>();
        if matches!(argv[1].as_str(), "import" | "export") {
            argv[3] = native
                .join(&argv[3])
                .to_str()
                .ok_or("Fixture path not Unicode")?
                .into();
        }
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera".to_owned()).chain(argv))?;
        let values = cli::reflection::values(&registry, &matches);
        let file = values["file"].as_str().map(Path::new);
        let seen = native.join("sdk.seen");
        if seen.exists() {
            std::fs::remove_file(&seen)?;
        }
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let mut outcome = services.execute_with_output(
            values.as_object().ok_or("Cookie arguments missing")?,
            1000.0,
            |_| panic!("Cookie fixture prompted"),
            || false,
            || panic!("Cookie fixture read stdin"),
            &mut output,
        )?;
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?.replace(
                native.to_str().ok_or("Cookie root not Unicode")?,
                "fixture-files"
            ),
            document["humanCookies"][index]["stdout"]
                .as_str()
                .ok_or("Cookie human source bytes missing")?
        );
        assert!(stderr.is_empty());
        if outcome.payload.get("file").is_some() {
            outcome.payload["file"] = file
                .and_then(|f| f.file_name())
                .and_then(|v| v.to_str())
                .ok_or("File name missing")?
                .into();
        }
        if outcome.payload["error"]
            .as_str()
            .is_some_and(|v| v.starts_with("no usable cookies found in "))
        {
            outcome.payload["error"] = format!(
                "no usable cookies found in {}",
                file.and_then(|f| f.file_name())
                    .and_then(|v| v.to_str())
                    .ok_or("File name missing")?
            )
            .into();
        }
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"], "{original:?}");
        if let Some(expected) = case["output"].as_str() {
            assert_eq!(
                std::fs::read_to_string(file.ok_or("Export file missing")?)?.replace("\r\n", "\n"),
                expected
            );
        }
        let activated = seen.exists();
        assert_eq!(activated, outcome.exit == 0);
        if activated {
            let calls: Value = serde_json::from_slice(&std::fs::read(&seen)?)?;
            assert_eq!(
                calls.as_array().and_then(|v| v.last()),
                Some(&json!("close"))
            );
            if values["cookies_command"] == "import" {
                assert_eq!(calls[2], "cookies-clear");
                assert_eq!(calls[3], "cookies-add");
            }
        }
        assert!(matches!(
            claims::read(&native, "fixture"),
            claims::Claim::Absent
        ));
    }
    assert_eq!(
        document["probes"]["tlsScript"],
        include_str!("../src/browser/tls_probe.js").trim()
    );
    assert_eq!(
        document["probes"]["dnsScript"],
        include_str!("../src/browser/dns_probe.js").trim()
    );
    let probes = document["probes"]["cases"]
        .as_array()
        .ok_or("Probe cases missing")?;
    assert_eq!(probes.len(), 10);
    for (index, case) in probes.iter().enumerate() {
        let actual = if case["operation"] == "dns" {
            numera_core::browser::probe_grades::dns(&case["observation"], case["country"].as_str())?
        } else {
            numera_core::browser::probe_grades::tls(&case["observation"])?
        };
        assert_eq!(actual, case["expected"], "Original probe grade {index}");
    }
    let observation = native.join("probe-observation.json");
    let script = native.join("probe-adapter.py");
    std::fs::write(
        &script,
        r#"
import json,pathlib,sys
seen=[];index=-1
case=json.loads(pathlib.Path(__file__).with_name('probe-observation.json').read_text(encoding='utf-8'))
for line in sys.stdin:
    request=json.loads(line);op=request['operation'];seen.append(request)
    if op=='hello': result={'backends':{engine:{'installed':True} for engine in ['playwright','camoufox','cloak']}}
    elif op=='open': result={'opened':True,'backend':request['backend']}
    elif op=='route': result={'routed':True}
    elif op=='navigate':
        index+=1
        result={'navigated':case.get('attempts',[{}]*3)[index].get('navigate',True)}
    elif op=='evaluate':
        if case.get('alignment') and request['expression'].startswith('(fonts)'):
            for directory in ['profiles','dashboard']:
                path=pathlib.Path(__file__).with_name(directory).joinpath('fixture.json')
                current=json.loads(path.read_text(encoding='utf-8'))
                current['notes']='edited during probe'
                path.write_text(json.dumps(current),encoding='utf-8')
            attempt={'value':case['observation']['fontsRendered']}
        elif 'observations' in case:
            expression=request['expression']
            kind='info' if 'async (urls)' in expression else 'ice' if 'RTCPeerConnection' in expression else 'dns' if 'idUrl' in expression else 'tls'
            attempt={'evaluate':False} if case['observations'].get('ipFailed') else {'value':case['observations'][kind]}
        else:
            attempt=case['attempts'][index] if 'attempts' in case else {'value':case['observation']}
        result={'evaluated':attempt.get('evaluate',True),'value':attempt.get('value')}
    elif op=='close': result={'closing':True}
    else: raise AssertionError('Unexpected native probe operation')
    pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
    )?;
    let browser = Runtime {
        adapter: script.clone(),
        ..browser.clone()
    };
    let services = Application {
        browser: Some(&browser),
        ..services
    };
    for case in probes {
        let mut profile = repo
            .resolve("fixture", 1000.0)?
            .ok_or("Probe profile missing")?;
        profile.proxy = case["country"]
            .as_str()
            .map(|country| {
                numera_core::proxy::Proxy::from_record(
                    &json!({"server":"http://proxy.fixture.test:8080","country":country,
                    "username":null,"password":null}),
                    false,
                )
            })
            .transpose()?;
        repo.save(profile, 1000.0)?;
        std::fs::write(&observation, serde_json::to_vec(case)?)?;
        let verb = case["operation"].as_str().ok_or("Probe verb missing")?;
        let outcome = probe_output::run(
            &document["humanProbes"],
            verb,
            &case["expected"],
            &services,
            None,
            false,
        )?;
        assert_eq!(outcome.payload, case["expected"]);
        assert_eq!(outcome.exit, i32::from(case["expected"]["ok"] != true));
        let calls: Value = serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        assert_eq!(calls[1]["options"]["headless"], true);
        let evaluation = calls
            .as_array()
            .ok_or("Probe calls missing")?
            .iter()
            .find(|call| call["operation"] == "evaluate")
            .ok_or("Probe never evaluated")?;
        assert_eq!(evaluation["tolerate_failure"], true);
        if verb == "dns" {
            assert_eq!(evaluation["argument"]["n"], 8);
        } else {
            assert_eq!(calls[2]["url"], "https://tls.peet.ws/api/all");
            assert_eq!(calls[2]["timeout_ms"], 20000);
        }
        assert_eq!(
            calls
                .as_array()
                .and_then(|calls| calls.last())
                .ok_or("Closing call missing")?["operation"],
            "close"
        );
        assert!(matches!(
            claims::read(&native, "fixture"),
            claims::Claim::Absent
        ));
    }
    let retries = document["probes"]["retries"]
        .as_array()
        .ok_or("TLS retries missing")?;
    assert_eq!(retries.len(), 3);
    for case in retries {
        std::fs::write(&observation, serde_json::to_vec(case)?)?;
        let matches = registry
            .clone()
            .try_get_matches_from(["numera", "tls", "fixture", "--json"])?;
        let values = cli::reflection::values(&registry, &matches);
        let outcome = services.execute(
            values.as_object().ok_or("Probe arguments missing")?,
            1000.0,
            |_| panic!("TLS retry confirmation"),
        )?;
        assert_eq!(outcome.payload, case["expected"]);
        assert_eq!(outcome.exit, i32::from(case["expected"]["ok"] != true));
        let calls: Value = serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        let calls = calls.as_array().ok_or("Retry calls missing")?;
        let navigation = calls
            .iter()
            .filter(|call| call["operation"] == "navigate")
            .collect::<Vec<_>>();
        let expected = case["calls"]
            .as_array()
            .ok_or("Original retry calls missing")?;
        assert_eq!(navigation.len(), expected.len());
        for (actual, expected) in navigation.iter().zip(expected) {
            assert_eq!(actual["url"], expected["url"]);
            assert_eq!(actual["timeout_ms"], expected["timeout"]);
            assert_eq!(actual["tolerate_failure"], true);
        }
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["operation"] == "evaluate")
                .count(),
            expected
                .iter()
                .filter(|call| call["evaluated"] == true)
                .count()
        );
        assert_eq!(
            calls.last().ok_or("Retry close missing")?["operation"],
            "close"
        );
        assert!(matches!(
            claims::read(&native, "fixture"),
            claims::Claim::Absent
        ));
    }
    let network = &document["probes"]["network"];
    assert_eq!(
        network["ipScript"],
        include_str!("../src/browser/ip_probe.js").trim()
    );
    assert_eq!(
        network["iceScript"],
        include_str!("../src/browser/webrtc_probe.js").trim()
    );
    for address in network["addresses"]
        .as_array()
        .ok_or("Address cases missing")?
    {
        let actual = numera_core::browser::network_grades::leaks(
            &json!({"ips":[address["ip"].clone()]}),
            None,
        )?;
        assert_eq!(
            actual["exposed"]
                .as_array()
                .ok_or("Exposed addresses missing")?
                .is_empty(),
            address["private"] == true
        );
    }
    let network_cases = network["cases"]
        .as_array()
        .ok_or("Network grades missing")?;
    assert_eq!(network_cases.len(), 5);
    assert_eq!(
        network["addresses"]
            .as_array()
            .ok_or("Address cases missing")?
            .len(),
        20
    );
    for case in network_cases
        .iter()
        .filter(|case| case["observations"]["ipFailed"] != true)
    {
        let mut profile = numera_core::models::Profile::from_record(&case["profile"])?;
        profile.id = "fixture".into();
        profile.name = "Fixture".into();
        let observations = &case["observations"];
        let leaks = numera_core::browser::network_grades::leaks(
            &observations["ice"],
            observations["info"]["ip"].as_str(),
        )?;
        let dns = numera_core::browser::probe_grades::dns(
            &observations["dns"],
            profile
                .proxy
                .as_ref()
                .and_then(|proxy| proxy.country.as_deref()),
        )?;
        let tls = numera_core::browser::probe_grades::tls(&observations["tls"])?;
        assert_eq!(
            numera_core::browser::network_grades::proxy(
                &profile,
                &observations["info"],
                &leaks,
                &dns,
                &tls,
                125
            )?,
            case["expected"]
        );
        repo.save(profile, 1000.0)?;
        std::fs::write(&observation, serde_json::to_vec(case)?)?;
        let matches = registry
            .clone()
            .try_get_matches_from(["numera", "proxy", "test", "fixture", "--json"])?;
        let values = cli::reflection::values(&registry, &matches);
        let mut outcome = services.execute(
            values.as_object().ok_or("Probe arguments missing")?,
            1000.0,
            |_| panic!("Proxy confirmation"),
        )?;
        assert!(outcome.payload["latencyMs"].as_u64().is_some());
        outcome.payload["latencyMs"] = json!(125);
        assert_eq!(outcome.payload, case["expected"]);
        assert_eq!(outcome.exit, i32::from(case["expected"]["ok"] != true));
        let calls: Value = serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        let calls = calls.as_array().ok_or("Network calls missing")?;
        let evaluations = calls
            .iter()
            .filter(|call| call["operation"] == "evaluate")
            .collect::<Vec<_>>();
        assert_eq!(evaluations.len(), 4);
        for (call, expression) in evaluations.iter().zip([
            include_str!("../src/browser/ip_probe.js"),
            include_str!("../src/browser/webrtc_probe.js"),
            include_str!("../src/browser/dns_probe.js"),
            include_str!("../src/browser/tls_probe.js"),
        ]) {
            assert_eq!(call["expression"], expression);
        }
        assert_eq!(
            calls
                .iter()
                .filter(|call| call["operation"] == "open")
                .count(),
            1
        );
        assert_eq!(
            calls.last().ok_or("Network close missing")?["operation"],
            "close"
        );
        assert!(matches!(
            claims::read(&native, "fixture"),
            claims::Claim::Absent
        ));
    }
    let failed = &network_cases[4];
    let mut profile = numera_core::models::Profile::from_record(&failed["profile"])?;
    profile.id = "fixture".into();
    repo.save(profile, 1000.0)?;
    std::fs::write(&observation, serde_json::to_vec(failed)?)?;
    let matches = registry
        .clone()
        .try_get_matches_from(["numera", "proxy", "test", "fixture", "--json"])?;
    let values = cli::reflection::values(&registry, &matches);
    let outcome = services.execute(
        values.as_object().ok_or("Probe arguments missing")?,
        1000.0,
        |_| panic!("Failed probe confirmation"),
    )?;
    assert_eq!(outcome.exit, 1);
    let mut expected = failed["expected"].clone();
    assert_eq!(expected["error"], "Synthetic IP SDK failure");
    // Raw SDK messages may contain credentials; compare the deliberate safe error.
    expected["error"] = "Browser IP evaluation failed".into();
    assert_eq!(outcome.payload, expected);
    let calls: Value = serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
    assert_eq!(
        calls
            .as_array()
            .ok_or("Failed probe calls missing")?
            .iter()
            .map(|call| call["operation"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        ["hello", "open", "evaluate", "close"]
    );
    assert!(matches!(
        claims::read(&native, "fixture"),
        claims::Claim::Absent
    ));
    let trust = &document["probes"]["trust"];
    assert_eq!(
        trust["script"],
        include_str!("../src/browser/trust_probe.js").trim()
    );
    let trust_cases = trust["cases"].as_array().ok_or("Trust cases missing")?;
    assert_eq!(trust_cases.len(), 5);
    for case in trust_cases {
        let mut profile = numera_core::models::Profile::from_record(&case["profile"])?;
        profile.id = "fixture".into();
        let engine = case["engine"].as_str().ok_or("Trust engine missing")?;
        assert_eq!(
            numera_core::browser::trust_grades::grade(
                &profile.fingerprint,
                &case["observation"],
                engine
            )?,
            case["expected"]
        );
        repo.save(profile, 1000.0)?;
        std::fs::write(&observation, serde_json::to_vec(case)?)?;
        for json_mode in [false, true] {
            let outcome = probe_output::run(
                &document["humanProbes"],
                "trust",
                &case["expected"],
                &services,
                Some(engine),
                json_mode,
            )?;
            assert_eq!(outcome.payload, case["expected"]);
            assert_eq!(
                outcome.exit,
                if json_mode {
                    0
                } else {
                    i32::from(case["expected"]["score"] != 100)
                }
            );
            let calls: Value =
                serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
            assert_eq!(
                calls
                    .as_array()
                    .ok_or("Trust calls missing")?
                    .iter()
                    .map(|call| call["operation"].as_str().unwrap_or(""))
                    .collect::<Vec<_>>(),
                ["hello", "open", "route", "navigate", "evaluate", "close"]
            );
            assert_eq!(calls[2]["pattern"], "**/*");
            assert_eq!(calls[2]["body"], trust["html"]);
            assert_eq!(calls[3]["url"], trust["url"]);
            assert_eq!(calls[3]["timeout_ms"], 15000);
            assert_eq!(
                calls[4]["expression"],
                include_str!("../src/browser/trust_probe.js")
            );
            assert!(matches!(
                claims::read(&native, "fixture"),
                claims::Claim::Absent
            ));
        }
    }
    let alignment = &document["alignment"];
    assert_eq!(
        alignment["fontScript"],
        include_str!("../src/browser/font_probe.js").trim()
    );
    assert_eq!(
        json!(numera_core::browser::alignment::candidates()?),
        alignment["candidates"]
    );
    let cases = alignment["cases"]
        .as_array()
        .ok_or("Alignment cases missing")?;
    assert_eq!(cases.len(), 3);
    for case in cases {
        let profile = numera_core::models::Profile::from_record(&case["profile"])?;
        repo.save(profile, 1000.0)?;
        let mut fixture = case.clone();
        fixture["alignment"] = true.into();
        std::fs::write(&observation, serde_json::to_vec(&fixture)?)?;
        let outcome = probe_output::run(
            &document["humanProbes"],
            "align",
            &case["expected"],
            &services,
            None,
            true,
        )?;
        assert_eq!(outcome.payload, case["expected"]);
        assert_eq!(outcome.exit, 0);
        let saved = repo
            .resolve("fixture", 1000.0)?
            .ok_or("Aligned profile missing")?;
        assert_eq!(saved.notes, "edited during probe");
        assert_eq!(
            serde_json::to_value(&saved.fingerprint)?,
            case["stored"]["fingerprint"]
        );
        let calls: Value = serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        assert_eq!(
            calls
                .as_array()
                .ok_or("Alignment calls missing")?
                .iter()
                .map(|call| call["operation"].as_str().unwrap_or(""))
                .collect::<Vec<_>>(),
            [
                "hello", "open", "route", "navigate", "evaluate", "evaluate", "close"
            ]
        );
        assert_eq!(
            calls[5]["expression"],
            include_str!("../src/browser/font_probe.js")
        );
        assert_eq!(calls[5]["argument"], alignment["candidates"]);
        assert!(matches!(
            claims::read(&native, "fixture"),
            claims::Claim::Absent
        ));
    }
    assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
