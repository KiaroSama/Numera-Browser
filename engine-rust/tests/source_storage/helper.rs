//! Native SDK process fixture used by scheduler lifecycle source cases.
use std::{
    fs,
    io::{self, BufRead, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn adapter(root: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    fs::write(root.join("adapter-pid"), std::process::id().to_string())?;
    let mode = fs::read_to_string(root.join("adapter-mode")).unwrap_or_else(|_| "normal".into());
    if mode.trim() == "exit-immediate" {
        std::process::exit(1);
    }
    let watched = root.to_owned();
    std::thread::spawn(move || {
        loop {
            if let Ok(code) = fs::read_to_string(watched.join("adapter-exit")) {
                fs::write(watched.join("adapter-exiting"), b"exit requested")
                    .expect("owned exit rendezvous");
                std::process::exit(code.trim().parse().unwrap_or(3));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let mut browser = None;
    for line in io::stdin().lock().lines() {
        let request: serde_json::Value = serde_json::from_str(&line?)?;
        let operation = request["operation"].as_str().ok_or("operation absent")?;
        let result = match operation {
            "hello" => {
                serde_json::json!({"backends":{"playwright":{"installed":true,"version":"native-fixture"},"patchright":{"installed":true,"version":"native-fixture"},"camoufox":{"installed":true,"version":"native-fixture"},"cloak":{"installed":true,"version":"native-fixture"}},"default_addons":[]})
            }
            "open" if mode.trim() == "fail-open" => {
                println!(
                    "{}",
                    serde_json::json!({"protocol":1,"id":request["id"],"ok":false,"error":{"kind":"sdk","type":"FixtureContextFailure"}})
                );
                io::stdout().flush()?;
                continue;
            }
            "open" => {
                let claims = fs::read_dir(root.join("sessions"))?
                    .filter_map(Result::ok)
                    .filter_map(|entry| fs::read(entry.path()).ok())
                    .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .collect::<Vec<_>>();
                fs::write(
                    root.join("claims-before-open.json"),
                    serde_json::to_vec(&claims)?,
                )?;
                let mut child = Command::new(std::env::current_exe()?)
                    .arg("leaf")
                    .arg(root.join("browser-pid"))
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                wait(&root.join("browser-pid"))?;
                fs::write(root.join("page-created"), b"page")?;
                fs::write(
                    root.join("private-env"),
                    serde_json::to_vec(
                        &serde_json::json!({"update_token":std::env::var_os("NUMERA_UPDATE_TOKEN").is_some(),"update_ticket":std::env::var_os("NUMERA_UPDATE_TICKET").is_some(),"update_activation":std::env::var_os("NUMERA_UPDATE_ACTIVATION").is_some()}),
                    )?,
                )?;
                let _ = child.try_wait()?;
                browser = Some(child);
                serde_json::json!({"opened":true,"backend":request["backend"]})
            }
            "navigate" => serde_json::json!({"navigated":true}),
            "route" => serde_json::json!({"routed":true}),
            "mouse" => serde_json::json!({"performed":true}),
            "evaluate" => {
                serde_json::json!({"evaluated":true,"value":fs::read(root.join("evaluation.json")).ok().and_then(|bytes|serde_json::from_slice::<serde_json::Value>(&bytes).ok())})
            }
            "inspect" => {
                serde_json::json!({"url":"about:blank","user_agent":"native-fixture-agent"})
            }
            "cookies-read" => serde_json::json!({"cookies":[]}),
            "cookies-clear" => serde_json::json!({"cleared":true}),
            "cookies-add" => serde_json::json!({"added":true}),
            "close" => {
                if mode.trim() != "fail-close" {
                    if let Some(mut child) = browser.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
                serde_json::json!({"closing":mode.trim()!="fail-close"})
            }
            _ => return Err(io::Error::other("Unknown native fixture operation").into()),
        };
        println!(
            "{}",
            serde_json::json!({"protocol":1,"id":request["id"],"ok":true,"result":result})
        );
        io::stdout().flush()?;
        if operation == "close" {
            return Ok(0);
        }
    }
    if let Some(mut child) = browser {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(0)
}
fn wait(path: &Path) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(30);
    while !path.is_file() {
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Fixture rendezvous expired",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "leaf") {
        fs::write(
            args.get(1).ok_or("leaf path")?,
            std::process::id().to_string(),
        )?;
        loop {
            thread::park_timeout(Duration::from_secs(1));
        }
    }
    adapter(&std::env::current_dir()?)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(111)
        }
    }
}
