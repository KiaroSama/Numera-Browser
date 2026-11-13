//! Compiled native SDK peers for CLI and HTTP cookie/probe contracts.
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
fn read(path: &Path) -> io::Result<Value> {
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}
pub fn run(mode: &str, config: Value) -> io::Result<()> {
    let case = if let Some(path) = config["case_file"].as_str() {
        read(Path::new(path))?
    } else {
        config
            .get("case")
            .cloned()
            .unwrap_or_else(|| config.clone())
    };
    let base = config["base"].as_str().map(PathBuf::from);
    let seen_path = config["seen"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| base.as_ref().map(|p| p.with_extension("seen")))
        .ok_or_else(|| invalid("CLI SDK seen path missing"))?;
    let store = config["store"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| base.as_ref().and_then(|p| p.parent()).map(Path::to_owned));
    let strings = config["seen_style"] == "operations"
        || (config["seen_style"].is_null() && mode == "cli-cookie");
    let mut seen = Vec::new();
    let mut index = None::<usize>;
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?).map_err(io::Error::other)?;
        let op = request["operation"]
            .as_str()
            .ok_or_else(|| invalid("SDK operation missing"))?;
        seen.push(if strings { json!(op) } else { request.clone() });
        let result = match op {
            "hello" => config.get("metadata").cloned().unwrap_or_else(|| {
                json!({"backends":{"playwright":{"installed":true},"camoufox":{"installed":true},"cloak":{"installed":true}}})
            }),
            "open" => json!({"opened":true,"backend":request["backend"]}),
            "cookies-read" => {
                let rows = if let Some(path) = config["cookies_file"].as_str() {
                    read(Path::new(path))?
                } else {
                    config.get("cookies").cloned().unwrap_or_else(|| json!([]))
                };
                json!({"cookies":rows})
            }
            "cookies-clear" => json!({"cleared":true}),
            "cookies-add" => json!({"added":true}),
            "route" => json!({"routed":true}),
            "navigate" => {
                index = Some(index.map_or(0, |n| n + 1));
                let attempt = &case["attempts"][index.unwrap_or(0)];
                json!({"navigated":attempt.get("navigate").cloned().unwrap_or(json!(true))})
            }
            "evaluate" => {
                let expression = request["expression"]
                    .as_str()
                    .ok_or_else(|| invalid("SDK expression missing"))?;
                let attempt = if case["alignment"] == true && expression.starts_with("(fonts)") {
                    let store = store.as_ref().ok_or_else(|| invalid("Alignment store missing"))?;
                    let id = config["profile_id"].as_str().unwrap_or("fixture");
                    for directory in ["profiles", "dashboard"] {
                        let path = store.join(directory).join(format!("{id}.json"));
                        let mut current = read(&path)?;
                        current["notes"] = "edited during probe".into();
                        fs::write(path, serde_json::to_vec(&current).map_err(io::Error::other)?)?;
                    }
                    json!({"value":case["observation"]["fontsRendered"]})
                } else if case.get("observations").is_some() {
                    let kind = if expression.contains("async (urls)") {
                        "info"
                    } else if expression.contains("RTCPeerConnection") {
                        "ice"
                    } else if expression.contains("idUrl") {
                        "dns"
                    } else {
                        "tls"
                    };
                    if case["observations"]["ipFailed"] == true {
                        json!({"evaluate":false})
                    } else {
                        json!({"value":case["observations"][kind]})
                    }
                } else if case.get("attempts").is_some() {
                    case["attempts"][index.ok_or_else(|| invalid("Probe evaluated before navigation"))?].clone()
                } else {
                    json!({"value":case["observation"]})
                };
                json!({"evaluated":attempt.get("evaluate").cloned().unwrap_or(json!(true)),"value":attempt["value"]})
            }
            "close" => json!({"closing":true}),
            _ => return Err(invalid("Unexpected CLI SDK operation")),
        };
        fs::write(
            &seen_path,
            serde_json::to_vec(&seen).map_err(io::Error::other)?,
        )?;
        let mut out = io::stdout().lock();
        writeln!(
            out,
            "{}",
            json!({"protocol":1,"id":request["id"],"ok":true,"result":result})
        )?;
        out.flush()?;
        if op == "close" {
            break;
        }
    }
    Ok(())
}
