use numera_core::{
    browser::{
        admission, cookies,
        launch::{self, PreparedLaunch},
    },
    models::{Fingerprint, Profile},
    ownership::{claims, registry::Registry},
};
use serde_json::{Map, Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;
#[path = "common/hint_site.rs"]
mod hint_site;

fn assert_page(view: &Value, fingerprint: &Fingerprint, config: &Value, secure: bool) {
    assert_eq!(view["platform"], fingerprint.platform);
    assert_eq!(view["cores"], fingerprint.hardware_concurrency);
    assert_eq!(view["languages"], json!(fingerprint.languages));
    assert_eq!(view["userAgent"], fingerprint.user_agent);
    if secure || !fingerprint.is_mobile {
        assert_eq!(view["width"], fingerprint.viewport_width);
        assert_eq!(view["height"], fingerprint.viewport_height);
    }
    let data = &view["userAgentData"];
    if !secure {
        assert!(
            data.is_null(),
            "Blank page unexpectedly exposed secure-context UAData"
        );
        return;
    }
    if config["hasUaData"] == false {
        assert!(data.is_null(), "iOS exposed navigator.userAgentData");
    } else {
        for (key, expected) in [
            ("brands", &config["uaBrands"]),
            ("platform", &config["uaPlatform"]),
            ("platformVersion", &config["platformVersion"]),
            ("architecture", &config["architecture"]),
            ("model", &config["deviceModel"]),
            ("mobile", &config["isMobile"]),
            ("uaFullVersion", &config["fullVersion"]),
        ] {
            assert_eq!(&data[key], expected, "Page hint {key}");
        }
        assert_eq!(data["bitness"], "64");
        assert_eq!(data["wow64"], false);
    }
}

fn assert_wire(
    site: &hint_site::HintSite,
    prefix: &str,
    config: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let first = site.headers(&format!("{prefix}/first"))?;
    let second = site.headers(&format!("{prefix}/second"))?;
    let resource = site.headers(&format!("{prefix}/resource"))?;
    if config["hasUaData"] == false {
        assert!(first.is_empty(), "iOS sent first-request hints");
        assert!(second.is_empty(), "iOS sent hints after Accept-CH");
        assert!(resource.is_empty(), "iOS subresource sent hints");
    } else {
        assert_eq!(
            first.keys().map(String::as_str).collect::<Vec<_>>(),
            ["sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"],
            "High entropy hints were sent before Accept-CH"
        );
        for key in ["sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"] {
            assert_eq!(first.get(key), second.get(key), "Low hint changed: {key}");
        }
        for (key, value) in [
            ("sec-ch-ua-platform", &config["uaPlatform"]),
            ("sec-ch-ua-platform-version", &config["platformVersion"]),
            ("sec-ch-ua-arch", &config["architecture"]),
            ("sec-ch-ua-model", &config["deviceModel"]),
        ] {
            let expected = serde_json::to_string(value)?;
            assert_eq!(second.get(key), Some(&expected), "Wire hint {key}");
        }
        let mobile = if config["isMobile"] == true {
            "?1"
        } else {
            "?0"
        };
        assert_eq!(
            second.get("sec-ch-ua-mobile").map(String::as_str),
            Some(mobile)
        );
        assert_eq!(
            second.get("sec-ch-ua-bitness").map(String::as_str),
            Some("\"64\"")
        );
        assert_eq!(
            second.get("sec-ch-ua-wow64").map(String::as_str),
            Some("?0")
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires explicitly selected SDK and installed browser in the hosted live tier"]
fn selected_sdk_opens_real_persistent_context_in_native_owned_tree()
-> Result<(), Box<dyn std::error::Error>> {
    let backend = std::env::var("NUMERA_NATIVE_LIVE_ENGINE")
        .map_err(|_| "Selected live backend is required")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let uses_page_script = matches!(backend.as_str(), "playwright" | "patchright");
    let document: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Identity cases missing")?;
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| {
            case["input"]["seed"] == 1 && (uses_page_script || case["input"]["os"] == "windows")
        })
        .collect();
    assert_eq!(selected.len(), if uses_page_script { 5 } else { 1 });
    for case in selected {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        eprintln!("Browser stage: profile {}", fingerprint.os);
        let temporary = common::TempDirectory::new("native-browser-live")?;
        let root = temporary.path();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("Repository missing")?
            .join("scripts/browser-adapter/adapter.py");
        let mut command = Command::new(&python);
        command
            .arg("-u")
            .arg(script)
            .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
            .env("PYTHONUTF8", "1");
        let profile = Profile::from_record(&json!({
            "id":"a", "name":"fixture", "engine":backend,
            "created_at":1000.0, "updated_at":1000.0, "fingerprint":case["input"],
            "startup_urls":["invalid-fixture-url", "about:blank"],
        }))?;
        std::fs::create_dir_all(root.join("profiles"))?;
        std::fs::write(root.join("profiles/a.json"), serde_json::to_vec(&profile)?)?;
        let (session, profile) =
            admission::current(root, "a", "fixture", "fixture-host", None, 1000.0)?;
        eprintln!("Browser stage: native launch");
        let mut adapter = launch::open(
            session,
            profile,
            &mut command,
            PreparedLaunch {
                backend: &backend,
                headless: true,
                extra_args: &[],
                extensions: &[],
                country: None,
                relay: None,
                binary_options: Map::new(),
            },
            1000.0,
        )?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            assert_eq!(
                claims::state(&claims::read(root, "a"), "fixture-host"),
                claims::State::Busy
            );
            assert_eq!(
                adapter.call(
                    "navigate",
                    Map::from_iter([("url".into(), "about:blank".into())]),
                    Duration::from_secs(35)
                )?["navigated"],
                true
            );
            let view = adapter.call("inspect", Map::new(), Duration::from_secs(10))?;
            if uses_page_script {
                assert_page(&view, &fingerprint, &case["config"], false);
            }
            assert!(
                view["userAgent"]
                    .as_str()
                    .is_some_and(|agent| agent.starts_with("Mozilla/5.0"))
            );
            assert!(view["cores"].as_u64().is_some_and(|cores| cores > 0));
            if uses_page_script {
                for new_page in [false, true] {
                    eprintln!("Browser stage: loopback new_page={new_page}");
                    // A fresh origin is required for each first-request timing assertion.
                    let mut site = hint_site::HintSite::new()?;
                    let prefix = if new_page { "/new" } else { "/initial" };
                    for suffix in ["/first", "/second"] {
                        eprintln!("Browser stage: loopback navigation {suffix}");
                        assert_eq!(
                            adapter.call(
                                "navigate",
                                Map::from_iter([
                                    ("url".into(), site.url(&format!("{prefix}{suffix}"))?.into()),
                                    ("new_page".into(), (new_page && suffix == "/first").into()),
                                ]),
                                Duration::from_secs(35)
                            )?["navigated"],
                            true
                        );
                    }
                    eprintln!("Browser stage: wire assertions");
                    assert_wire(&site, prefix, &case["config"])?;
                    eprintln!("Browser stage: secure page inspection");
                    let view = adapter.call("inspect", Map::new(), Duration::from_secs(10))?;
                    assert_page(&view, &fingerprint, &case["config"], true);
                    eprintln!("Browser stage: site cleanup");
                    site.finish()?;
                    eprintln!("Browser stage: loopback finished");
                }
            }
            eprintln!("Browser stage: isolated cookie roundtrip");
            assert!(cookies::read(&mut adapter)?.is_empty());
            let cookie = json!({"name":"fixture", "value":"synthetic", "domain":".fixture.test", "path":"/", "expires":-1.0, "httpOnly":true, "secure":false, "sameSite":"Lax"});
            assert_eq!(
                adapter.call(
                    "cookies-add",
                    Map::from_iter([("cookies".into(), json!([cookie]))]),
                    Duration::from_secs(10)
                )?["added"],
                true
            );
            let rows = cookies::read(&mut adapter)?;
            assert_eq!(rows.len(), 1);
            for (key, expected) in cookie.as_object().ok_or("Cookie fixture missing")? {
                assert_eq!(&rows[0][key], expected, "Cookie field {key}");
            }
            assert_eq!(
                adapter.call("cookies-clear", Map::new(), Duration::from_secs(10))?["cleared"],
                true
            );
            assert!(cookies::read(&mut adapter)?.is_empty());
            assert!(root.join("user-data/a").is_dir());
            Ok(())
        })();
        let cleanup = if result.is_ok() {
            let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
            let foreign = Registry::new(root.into(), "fixture-host".into(), None)?;
            // Transfer the existing admitted SDK owner; no second lease can be acquired.
            let transferred = registry.retain_browser("a", adapter);
            transferred?;
            assert!(registry.owns("a")?);
            assert!(registry.running("a")?);
            assert!(!foreign.stop("a", Duration::from_secs(10))?);
            assert!(registry.stop("a", Duration::from_secs(10))?);
            assert!(!registry.owns("a")?);
            assert!(registry.shutdown(Duration::from_secs(10)).is_empty());
            Ok(())
        } else {
            adapter.finish(Duration::from_secs(10))
        };
        if result.is_err() || cleanup.is_err() {
            for entry in std::fs::read_dir(root.join("logs"))? {
                let path = entry?.path();
                if std::fs::metadata(&path)?.len() <= 65_536 {
                    eprintln!(
                        "Sanitized adapter diagnostics:\n{}",
                        std::fs::read_to_string(path)?
                    );
                }
            }
        }
        cleanup?;
        result?;
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let log = std::fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(log.len(), 1);
        let content = std::fs::read_to_string(log[0].path())?;
        assert!(content.contains("Persistent context opened"));
        assert!(content.contains("Finished exit=0"));
        assert!(
            !content.contains("[WARNING]"),
            "SDK configuration emitted a warning"
        );
        temporary.cleanup()?;
    }
    Ok(())
}
