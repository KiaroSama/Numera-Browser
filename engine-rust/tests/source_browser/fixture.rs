//! Compiled Rust vendor and process fixture. The native SDK and ownership code
//! execute normally; only the external vendor transport is supplied by this file.
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
fn send(v: Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(&v)?;
    let mut out = io::stdout().lock();
    out.write_all(&(bytes.len() as u32).to_le_bytes())?;
    out.write_all(&bytes)?;
    out.flush()
}
fn create(parent: &str, kind: &str, guid: &str, init: Value) -> io::Result<()> {
    send(
        json!({"guid":parent,"method":"__create__","params":{"type":kind,"guid":guid,"initializer":init}}),
    )
}
fn page(root: &Path) -> io::Result<()> {
    create(
        "context",
        "Frame",
        "frame",
        json!({"url":"about:blank","name":""}),
    )?;
    create(
        "context",
        "Page",
        "page",
        json!({"mainFrame":{"guid":"frame"},"viewportSize":{"width":800,"height":600},"isClosed":false}),
    )?;
    send(json!({"guid":"context","method":"page","params":{"page":{"guid":"page"}}}))?;
    fs::write(root.join("page-created"), b"page")
}
fn number(value: &Value) -> Value {
    value
        .as_f64()
        .filter(|n| {
            n.is_finite() && n.fract() == 0.0 && *n >= i64::MIN as f64 && *n < i64::MAX as f64
        })
        .map_or_else(|| value.clone(), |n| json!(n as i64))
}
// The JavaScript vendor serializes all numbers through JSON.stringify, so
// integral wire doubles return as JSON integers after an evaluation round trip.
fn javascript_json(value: &Value) -> Value {
    match value {
        Value::Number(_) => number(value),
        Value::Array(values) => Value::Array(values.iter().map(javascript_json).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), javascript_json(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn seen(root: &Path, rows: &mut Vec<Value>, value: Value) -> io::Result<()> {
    rows.push(value);
    fs::write(root.join("seen.json"), serde_json::to_vec(rows)?)
}
fn vendor(root: &Path) -> io::Result<()> {
    fs::write(root.join("driver-started"), b"started")?;
    fs::write(
        root.join("driver-policy.json"),
        json!({"auto_update":std::env::var("CLOAKBROWSER_AUTO_UPDATE").ok()}).to_string(),
    )?;
    let mode = fs::read_to_string(root.join("vendor-mode")).unwrap_or_default();
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("vendor-requests.jsonl"))?;
    let mut input = io::stdin().lock();
    let mut browser = None;
    let mut observed = Vec::new();
    let mut pending_pattern = None;
    let mut watcher = None;
    let watch_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    loop {
        let mut length = [0; 4];
        match input.read_exact(&mut length) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let size = u32::from_le_bytes(length) as usize;
        if size > 4 * 1024 * 1024 {
            return Err(io::Error::other("oversized request"));
        }
        let mut bytes = vec![0; size];
        input.read_exact(&mut bytes)?;
        let r: Value = serde_json::from_slice(&bytes)?;
        writeln!(log, "{r}")?;
        log.flush()?;
        let method = r["method"].as_str().unwrap_or("");
        let id = r["id"].clone();
        let failing = (mode == "probe"
            && ((method == "goto" && r["params"]["url"] == "https://failure.fixture.test/")
                || (method == "mouseWheel" && r["params"]["deltaY"].as_f64() == Some(-1.0))
                || (method == "evaluateExpression" && r["params"]["expression"] == "fail")))
            || (method == "launchPersistentContext" && mode == "launch-fail")
            || (method == "addInitScript"
                && matches!(
                    mode.as_str(),
                    "init-fail" | "init-suppress" | "cleanup-fail" | "cleanup-hang"
                ))
            || (method == "newPage" && mode == "page-fail");
        if mode == "probe" {
            match method {
                "goto" => seen(
                    root,
                    &mut observed,
                    json!({"operation":"goto","url":r["params"]["url"],"wait_until":r["params"]["waitUntil"],"timeout":number(&r["metadata"]["timeout"])}),
                )?,
                "mouseWheel" => seen(
                    root,
                    &mut observed,
                    json!({"operation":"wheel","x":number(&r["params"]["deltaX"]),"y":number(&r["params"]["deltaY"])}),
                )?,
                "mouseMove" => seen(
                    root,
                    &mut observed,
                    json!({"operation":"move","x":number(&r["params"]["x"]),"y":number(&r["params"]["y"]),"steps":r["params"]["steps"]}),
                )?,
                "evaluateExpression" => {
                    let argument =
                        numera_core::browser::sdk::values::decode(&r["params"]["arg"]["value"])
                            .map_err(io::Error::other)?;
                    seen(
                        root,
                        &mut observed,
                        json!({"operation":"evaluate","expression":r["params"]["expression"],"argument":argument}),
                    )?;
                }
                "close" => seen(root, &mut observed, json!({"operation":"close"}))?,
                _ => {}
            }
        }
        if failing {
            send(
                json!({"id":id,"error":{"error":{"message":if mode=="probe"{"Sensitive SDK detail must not escape"}else{"planted vendor failure private-profile-payload"},"name":"Error","stack":"planted","suppressed":mode=="init-suppress"}}}),
            )?;
            continue;
        }
        let result = match method {
            "initialize" => {
                for name in ["chromium", "firefox", "webkit"] {
                    create(
                        "",
                        "BrowserType",
                        name,
                        json!({"name":name,"executablePath":"fixture"}),
                    )?;
                }
                create(
                    "",
                    "Playwright",
                    "pw",
                    json!({"chromium":{"guid":"chromium"},"firefox":{"guid":"firefox"},"webkit":{"guid":"webkit"}}),
                )?;
                json!({"playwright":{"guid":"pw"}})
            }
            "launchPersistentContext" => {
                let child = Command::new(std::env::current_exe()?)
                    .arg("leaf")
                    .arg(root)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                fs::write(root.join("browser-pid"), child.id().to_string())?;
                browser = Some(child);
                create(
                    r["guid"].as_str().expect("browser contract"),
                    "BrowserContext",
                    "context",
                    json!({}),
                )?;
                fs::write(root.join("context-created"), b"context")?;
                if matches!(mode.as_str(), "probe" | "first-page") {
                    page(root)?;
                }
                if mode == "first-page" && watcher.is_none() {
                    let root = root.to_owned();
                    let stop = std::sync::Arc::clone(&watch_stop);
                    watcher = Some(std::thread::spawn(move || {
                        let end = std::time::Instant::now() + Duration::from_secs(10);
                        while !root.join("trigger-close").exists() {
                            if stop.load(std::sync::atomic::Ordering::Acquire) {
                                return Ok(());
                            }
                            if std::time::Instant::now() >= end {
                                return Err(io::Error::other("First-page trigger timed out"));
                            }
                            std::thread::park_timeout(Duration::from_millis(2));
                        }
                        send(json!({"guid":"page","method":"close","params":{}}))
                    }));
                }
                json!({"context":{"guid":"context"}})
            }
            "newPage" => {
                if mode == "probe" {
                    seen(root, &mut observed, json!({"operation":"new_page"}))?;
                }
                page(root)?;
                json!({"page":{"guid":"page"}})
            }
            "setNetworkInterceptionPatterns" => {
                let pattern = r["params"]["patterns"][0]["glob"].clone();
                seen(
                    root,
                    &mut observed,
                    json!({"operation":"route","pattern":pattern}),
                )?;
                create(
                    "page",
                    "Request",
                    "request",
                    json!({"url":"https://ok.fixture.test/","resourceType":"document","method":"GET","headers":[],"frame":{"guid":"frame"},"isNavigationRequest":false}),
                )?;
                create("request", "Route", "route", json!({}))?;
                send(json!({"guid":"page","method":"route","params":{"route":{"guid":"route"}}}))?;
                pending_pattern = Some(id);
                continue;
            }
            "fulfill" => {
                let content_type = r["params"]["headers"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["name"] == "content-type"))
                    .map(|row| row["value"].clone())
                    .unwrap_or(Value::Null);
                seen(
                    root,
                    &mut observed,
                    json!({"operation":"fulfill","status":r["params"]["status"],"content_type":content_type,"body":r["params"]["body"]}),
                )?;
                send(json!({"id":id,"result":{}}))?;
                if let Some(pending) = pending_pattern.take() {
                    send(json!({"id":pending,"result":{}}))?;
                }
                continue;
            }
            "evaluateExpression" => {
                if r["params"]["expression"] == "nonfinite" {
                    json!({"value":{"v":"NaN"}})
                } else {
                    json!({"value":{"o":[{"k":"argument","v":javascript_json(&r["params"]["arg"]["value"])}]}})
                }
            }
            "newCDPSession" => {
                create("context", "CDPSession", "cdp", json!({}))?;
                json!({"session":{"guid":"cdp"}})
            }
            "send" => json!({"value":{}}),
            "close" => {
                let data = root.parent().expect("browser contract");
                let claim_busy = matches!(
                    numera_core::ownership::claims::read(data, "browser-source"),
                    numera_core::ownership::claims::Claim::Record(_)
                );
                let lease_busy = numera_core::storage::locks::SessionLease::acquire(
                    &numera_core::storage::locks::SessionLease::path_for(data, "browser-source")?,
                )
                .is_err();
                fs::write(
                    root.join("close-owned.json"),
                    json!({"claim":claim_busy,"lease_busy":lease_busy}).to_string(),
                )?;
                if mode == "cleanup-hang" {
                    let mut rest = Vec::new();
                    input.read_to_end(&mut rest)?;
                    break;
                }
                if mode == "cleanup-fail" {
                    send(
                        json!({"id":id,"error":{"error":{"message":"context close refused","name":"Error"}}}),
                    )?;
                    continue;
                }
                fs::write(root.join("context-closed"), b"closed")?;
                if let Some(mut b) = browser.take() {
                    let _ = b.kill();
                    let _ = b.wait();
                }
                send(json!({"guid":"context","method":"close","params":{}}))?;
                json!({})
            }
            _ => json!({}),
        };
        send(json!({"id":id,"result":result}))?;
    }
    if let Some(mut b) = browser {
        if mode == "cleanup-hang" {
            // Deliberately leave the real descendant for the native owner's
            // failed-cleanup/retention boundary to observe and settle.
            std::mem::forget(b);
        } else {
            let _ = b.kill();
            let _ = b.wait();
        }
    }
    watch_stop.store(true, std::sync::atomic::Ordering::Release);
    if let Some(watcher) = watcher {
        watcher
            .join()
            .map_err(|_| io::Error::other("First-page watcher panicked"))??;
    }
    fs::write(root.join("driver-stopped"), b"stopped")?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.get(1).is_some_and(|a| a == "run-driver") {
        return Ok(vendor(
            Path::new(&args[0])
                .parent()
                .expect("browser contract")
                .parent()
                .expect("browser contract"),
        )?);
    }
    match args.first().and_then(|s| s.to_str()) {
        None => {
            let logs =
                std::env::var_os("NUMERA_ADAPTER_LOG_DIR").ok_or("missing wrapper log root")?;
            let root = Path::new(&logs).parent().ok_or("missing fixture root")?;
            let status = Command::new(env!("NUMERA_TEST_WORKER"))
                .env("NUMERA_PLAYWRIGHT_DRIVER", root.join("driver"))
                .env_remove("NUMERA_UPDATE_TOKEN")
                .env_remove("NUMERA_PASSWORD")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        Some("relay-failure") => {
            let root = Path::new(&args[1]);
            let mut p = numera_core::conversion::to_engine(
                &serde_json::from_str(r#"{"id":"relay-profile","name":"relay","seed":1}"#)?,
                1000.0,
            )?;
            p.proxy = Some(numera_core::proxy::Proxy::from_record(
                &serde_json::from_str(
                    r#"{"server":"socks5://127.0.0.1:9","username":"private-user","password":"private-secret"}"#,
                )?,
                true,
            )?);
            assert!(
                p.proxy
                    .as_ref()
                    .expect("authenticated proxy")
                    .to_playwright()
                    .get("username")
                    .is_some()
            );
            // Tokio's Unix signal self-pipe belongs to the process and survives
            // each runtime. Establish that subsystem before measuring one
            // relay/session lifecycle, then require zero additional resources.
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            drop(runtime);
            let guard = numera_core::ownership::resources::ResourceGuard::new()?;
            let session =
                numera_core::ownership::session::Session::admit(root, &p.id, "resource-fixture")?;
            let mut command = Command::new(root.join("missing-sdk-worker"));
            let result = numera_core::browser::launch::open(
                session,
                p.clone(),
                &mut command,
                numera_core::browser::launch::PreparedLaunch {
                    backend: "playwright",
                    headless: true,
                    extra_args: &[],
                    cwd: root,
                    home: root,
                    country: None,
                    relay: None,
                    binary_options: Default::default(),
                },
                1000.0,
            );
            assert!(result.is_err());
            guard.finish()?;
            assert!(matches!(
                numera_core::ownership::claims::read(root, &p.id),
                numera_core::ownership::claims::Claim::Absent
            ));
            Ok(())
        }
        Some("resource") => {
            let root = Path::new(&args[1]);
            let kind = args[2].to_str().expect("browser contract");
            let gated = args[3] == "on";
            let guard = numera_core::ownership::resources::ResourceGuard::new()?;
            match kind {
                "file" => {
                    let f = fs::File::create(root.join("left-open.txt"))?;
                    std::mem::forget(f);
                }
                "thread" => {
                    let _ = std::thread::spawn(|| panic!("planted native worker failure")).join();
                }
                "child" => {
                    let child = Command::new(std::env::current_exe()?)
                        .arg("leaf")
                        .arg(root)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                    fs::write(root.join("residual-child"), child.id().to_string())?;
                    std::mem::forget(child);
                }
                "clean" => {
                    fs::write(root.join("tidy.txt"), b"x")?;
                    let mut f = fs::File::open(root.join("tidy.txt"))?;
                    let mut b = Vec::new();
                    f.read_to_end(&mut b)?;
                    assert_eq!(b, b"x");
                    drop(f);
                }
                _ => return Err("unknown resource fixture".into()),
            }
            let verdict = guard.finish();
            if gated && verdict.is_err() {
                eprintln!("{}", verdict.expect_err("expected browser rejection"));
                std::process::exit(1);
            }
            Ok(())
        }
        Some("leaf") => loop {
            std::thread::park_timeout(Duration::from_secs(60));
        },
        Some("emit") => {
            let text = args[1].to_str().expect("browser contract");
            print!("{text}");
            io::stdout().flush()?;
            std::process::exit(args[2].to_str().expect("browser contract").parse()?);
        }
        Some("pipe") => {
            let mode = args[1].to_str().expect("browser contract");
            if mode == "arrival" {
                let mut b = [0];
                io::stdin().read_exact(&mut b)?;
            }
            if mode != "eof" && mode != "idle" {
                print!("{{\"value\":1}}\n");
                io::stdout().flush()?;
            }
            if mode == "idle" {
                let mut b = Vec::new();
                io::stdin().read_to_end(&mut b)?;
            }
            Ok(())
        }
        _ => Err("unknown Rust fixture mode".into()),
    }
}
