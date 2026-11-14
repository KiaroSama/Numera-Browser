//! Native RPC fixtures for the historical browser ownership/controller tests.
mod cli_browser_fixture;
mod http_process_fixture;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn emit(v: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{v}")?;
    out.flush()
}
fn write(path: Option<&str>, v: &Value) -> io::Result<()> {
    if let Some(path) = path {
        fs::write(path, serde_json::to_vec(v)?)?;
    }
    Ok(())
}
fn wait(path: &Path) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        if Instant::now() >= end {
            return Err(io::Error::other("Fixture rendezvous timed out"));
        }
        std::thread::park_timeout(Duration::from_millis(2));
    }
    Ok(())
}
fn present(config: &Value, key: &str) -> Option<String> {
    config[key].as_str().map(str::to_owned)
}
fn companion(config: &Value, key: &str, extension: &str) -> Option<String> {
    present(config, key).or_else(|| {
        config["base"].as_str().map(|base| {
            Path::new(base)
                .with_extension(extension)
                .to_string_lossy()
                .into_owned()
        })
    })
}
fn attach(
    port: u16,
    queries: Option<String>,
) -> io::Result<(Arc<AtomicBool>, std::thread::JoinHandle<io::Result<()>>)> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let mut query_count = 0;
        while !signal.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                    let mut request = [0; 4096];
                    let count = stream.read(&mut request)?;
                    if !String::from_utf8_lossy(&request[..count]).contains("/json/version") {
                        return Err(io::Error::other("Unexpected attach URL"));
                    }
                    query_count += 1;
                    if let Some(path) = &queries {
                        fs::write(path, query_count.to_string())?;
                    }
                    let body=json!({"Browser":"Fixture Chromium","webSocketDebuggerUrl":format!("ws://127.0.0.1:{port}/devtools/browser/fixture")}).to_string();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )?;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::park_timeout(Duration::from_millis(2))
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    });
    Ok((stop, thread))
}
fn rpc(mode: &str, config: Value) -> io::Result<()> {
    let mut seen = Vec::<Value>::new();
    let mut tab = 0;
    let mut server = None;
    let mut registered_child: Option<std::process::Child> = None;
    let mut rows = io::stdin().lock().lines();
    while let Some(line) = rows.next() {
        let request: Value = serde_json::from_str(&line?)?;
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| io::Error::other("RPC operation absent"))?;
        if mode == "wrong-envelope" {
            emit(&json!({"protocol":2,"id":1,"ok":true,"result":{}}))?;
            break;
        }
        if mode == "partial-frame" {
            print!("{{");
            io::stdout().flush()?;
            break;
        }
        if mode == "lifecycle" && config["invalid"] == true && operation == "open" {
            println!("not-json");
            io::stdout().flush()?;
            break;
        }
        let mut ok = true;
        if config["trace_operations"] == true {
            seen.push(json!(operation));
        }
        if mode == "registered" && operation == "hello" && config["fail_hello"] == true {
            let root = Path::new(
                config["root"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("registered root absent"))?,
            );
            let child = std::process::Command::new(std::env::current_exe()?)
                .arg("wait")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            fs::write(root.join("registered-child"), child.id().to_string())?;
            // Deliberately leave the real child alive after this malformed hello.
            // The native controller, rather than fixture teardown, must end it.
            emit(&json!({"protocol":2,"id":request["id"],"ok":true,"result":{}}))?;
            return Ok(());
        }
        if matches!(mode, "leader-exit-held-pipe" | "final-reply-held-pipe")
            && operation == "navigate"
        {
            let root = Path::new(
                config["root"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("held-pipe fixture root absent"))?,
            );
            // This real survivor deliberately retains the producer's stdout.
            // The controller must observe leader death independently of EOF,
            // then end this same owned tree during its normal outer cleanup.
            let child = std::process::Command::new(std::env::current_exe()?)
                .arg("wait")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            fs::write(root.join("held-pipe-child"), child.id().to_string())?;
            if mode == "final-reply-held-pipe" {
                emit(
                    &json!({"protocol":1,"id":request["id"],"ok":true,"result":{"navigated":true}}),
                )?;
            }
            std::process::exit(3);
        }
        let result = match operation {
            "hello" => {
                if mode == "cookies" {
                    seen.push(json!({"operation":"open"}));
                } else if mode == "cli" {
                    seen.push(json!(operation));
                }
                config.get("metadata").cloned().unwrap_or_else(||json!({"backends":{"playwright":{"installed":true},"patchright":{"installed":true},"camoufox":{"installed":true},"cloak":{"installed":true}}}))
            }
            "open" => {
                if mode == "registered" {
                    let root = Path::new(
                        config["root"]
                            .as_str()
                            .ok_or_else(|| io::Error::other("registered root absent"))?,
                    );
                    let id = config["profile"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("registered profile absent"))?;
                    if !root.join("sessions").join(format!("{id}.json")).is_file() {
                        return Err(io::Error::other("registered open preceded its claim"));
                    }
                    fs::write(root.join("registered-claim-before-open"), b"published")?;
                    let child = std::process::Command::new(std::env::current_exe()?)
                        .arg("wait")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;
                    fs::write(root.join("registered-child"), child.id().to_string())?;
                    registered_child = Some(child);
                }

                if let Some(path) = present(&config, "headless_capture") {
                    write(Some(&path), &request["options"]["headless"])?;
                }
                if let Some(capture) = present(&config, "capture") {
                    let value = if config["capture_environment"] == true {
                        json!({"frame":request,"geoip":std::env::var("NUMERA_ADAPTER_GEOIP_DIR").ok()})
                    } else {
                        request.clone()
                    };
                    write(Some(&capture), &value)?;
                }
                if mode == "cli" {
                    seen.push(json!(operation));
                    write(
                        companion(&config, "policy", "policy").as_deref(),
                        &json!({"backend":request["backend"],"first":std::env::var("NUMERA_ADAPTER_FIRST_PAGE").ok()}),
                    )?;
                }
                if mode == "attach" {
                    let arg = request["options"]["args"]
                        .as_array()
                        .and_then(|args| {
                            args.iter()
                                .filter_map(Value::as_str)
                                .find_map(|arg| arg.strip_prefix("--remote-debugging-port="))
                        })
                        .ok_or_else(|| io::Error::other("Attach port absent"))?;
                    server = Some(attach(
                        arg.parse().map_err(io::Error::other)?,
                        present(&config, "queries"),
                    )?);
                    let value = if config["capture_with_first"] == true {
                        json!({"request":request,"first":std::env::var("NUMERA_ADAPTER_FIRST_PAGE").ok()})
                    } else {
                        request.clone()
                    };
                    write(companion(&config, "capture", "open").as_deref(), &value)?;
                }
                json!({"opened":true,"backend":request["backend"]})
            }
            "cookies-read" => json!({"cookies":[]}),
            "cookies-clear" => {
                seen.push(json!({"operation":"clear"}));
                ok = config["failure"] != "clear";
                json!({"cleared":true})
            }
            "cookies-add" => {
                seen.push(json!({"operation":"add","cookies":request["cookies"]}));
                ok = config["failure"] != "add";
                json!({"added":true})
            }
            "navigate" => {
                if mode == "startup" {
                    assert_eq!(request["tolerate_failure"], true);
                    if request["new_page"] == true {
                        tab += 1;
                    }
                    let index = seen.len();
                    seen.push(json!({"tab":tab,"url":request["url"],"wait_until":"domcontentloaded","timeout":30000}));
                    json!({"navigated":!config["failed"].as_array().is_some_and(|rows|rows.contains(&json!(index)))})
                } else {
                    seen.push(json!({"operation":"navigate","url":request["url"],"timeout":request["timeout_ms"]}));
                    if config["failed"] == true {
                        if let Some(path) = companion(&config, "ticks", "ticks") {
                            fs::write(path, seen.len().to_string())?;
                        }
                    }
                    json!({"navigated":config["failed"]!=true})
                }
            }
            "mouse" => {
                seen.push(json!({"operation":request["action"],"x":request["x"],"y":request["y"]}));
                json!({"performed":true})
            }
            "evaluate" => {
                json!({"evaluated":true,"value":config.get("evaluation").cloned().unwrap_or_else(||if request["argument"].is_null(){json!("fixture.test")}else{json!(["https://fixture.test/deeper"])}) })
            }
            "close" => {
                if let Some(mut child) = registered_child.take() {
                    let _ = child.kill();
                    child.wait()?;
                }

                if mode == "cookies" {
                    seen.push(json!({"operation":"close"}));
                } else if mode == "cli" {
                    seen.push(json!(operation));
                }
                json!({"closing":true})
            }
            _ => json!({}),
        };
        write(companion(&config, "seen", "seen").as_deref(), &json!(seen))?;
        emit(&json!({"protocol":1,"id":request["id"],"ok":ok,"result":result}))?;
        if operation == "close" {
            if let Some(ack) = present(&config, "ack") {
                fs::write(ack, b"ack")?;
                wait(Path::new(
                    config["release"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Release path absent"))?,
                ))?;
                fs::write(
                    config["completed"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Completed path absent"))?,
                    b"finished",
                )?;
            }
            break;
        }
        if operation == "open" && mode == "attach" && config["natural"] == true {
            let release = companion(&config, "release", "release")
                .ok_or_else(|| io::Error::other("Natural close path absent"))?;
            wait(Path::new(&release))?;
            break;
        }
    }
    if let Some((stop, thread)) = server {
        stop.store(true, Ordering::Release);
        thread
            .join()
            .map_err(|_| io::Error::other("Attach fixture thread panicked"))??;
    }
    Ok(())
}
fn main() -> io::Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let own_config = std::env::current_exe()?.with_extension("fixture.json");
    if own_config.is_file() {
        let config: Value = serde_json::from_slice(&fs::read(&own_config)?)?;
        if http_process_fixture::dispatch(&config, &args)? {
            return Ok(());
        }
    }
    if args.first().is_some_and(|arg| arg == "--metadata") {
        let config: Value = serde_json::from_slice(&fs::read(&own_config)?)?;
        return emit(&config["metadata"]);
    }
    let (mode, config) = if let Some(mode) = args.first() {
        let mode = mode
            .to_str()
            .ok_or_else(|| io::Error::other("Mode not text"))?
            .to_owned();
        let config = if let Some(file) = args.get(1) {
            serde_json::from_slice(&fs::read(file)?)?
        } else {
            json!({})
        };
        (mode, config)
    } else {
        let exe = std::env::current_exe()?;
        let config: Value = serde_json::from_slice(&fs::read(exe.with_extension("fixture.json"))?)?;
        (
            config["mode"]
                .as_str()
                .ok_or_else(|| io::Error::other("Configured mode absent"))?
                .to_owned(),
            config,
        )
    };
    if matches!(mode.as_str(), "cli-cookie" | "cli-probe") {
        return cli_browser_fixture::run(&mode, config);
    }
    if mode == "wait" {
        loop {
            std::thread::park_timeout(Duration::from_secs(60));
        }
    }
    if mode == "frames" {
        let chunks: Vec<Vec<u8>> = serde_json::from_value(config["chunks"].clone())?;
        let mut input = io::stdin().lock();
        let mut byte = [0];
        fs::write(
            config["ready"]
                .as_str()
                .ok_or_else(|| io::Error::other("Ready marker missing"))?,
            b"ready",
        )?;
        input.read_exact(&mut byte)?;
        for (index, chunk) in chunks.iter().enumerate() {
            io::stdout().write_all(chunk)?;
            io::stdout().flush()?;
            if chunks.len() == 2 && index == 0 {
                fs::write(
                    config["partial"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Partial marker missing"))?,
                    b"partial",
                )?;
                input.read_exact(&mut byte)?;
            }
        }
        return Ok(());
    }
    rpc(&mode, config)
}
