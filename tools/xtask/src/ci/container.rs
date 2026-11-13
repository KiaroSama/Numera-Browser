//! Native image smoke orchestration with an explicitly named, retained Docker
//! owner. No host browser/cache mount or network reaches the acceptance probe.
use super::{Result, capture, fail, owned, workflow};
use std::{
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub fn smoke(root: &Path, image: &str, version: &str) -> Result<()> {
    if !numera_core::browser::container_smoke::exact_version(version) {
        return fail("Image must declare an exact Playwright release");
    }
    if image.is_empty()
        || image.len() > 200
        || image.chars().any(char::is_whitespace)
        || image.starts_with('-')
    {
        return fail("Image name is invalid");
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let name = format!("numera-browser-smoke-{}-{nonce}", std::process::id());
    let mut command = smoke_command(image, version, &name);
    let result = owned::run(
        root,
        &mut command,
        owned::limits(
            Duration::from_secs(60),
            Duration::from_secs(30),
            1024 * 1024,
        ),
    );
    settle(&name)?;
    let output = result?.command;
    if output.exit != 0 {
        return fail("Native image browser smoke failed");
    }
    Ok(())
}
pub fn smoke_command(image: &str, version: &str, name: &str) -> Command {
    let mut command = Command::new("docker");
    command.args([
        "run",
        "--rm",
        "--init",
        "--network",
        "none",
        "--shm-size=1g",
        "--name",
        name,
        "--env",
        &format!("NUMERA_CONTAINER_PLAYWRIGHT_VERSION={version}"),
        "--entrypoint",
        "/app/numera-browser-runtime",
        image,
        "--container-smoke",
    ]);
    command
}
fn settle(name: &str) -> Result<()> {
    // A killed CLI is not proof that its daemon-owned container stopped.
    let _ = capture(
        Command::new("docker").args(["rm", "-f", name]),
        Duration::from_secs(15),
        Duration::from_secs(10),
        65536,
    );
    let absent = capture(
        Command::new("docker").args([
            "ps",
            "-a",
            "--format",
            "{{.Names}}",
            "--filter",
            &format!("name=^/{name}$"),
        ]),
        Duration::from_secs(15),
        Duration::from_secs(10),
        65536,
    )?;
    if absent.exit != 0 || !absent.stdout.is_empty() {
        return fail("Owned browser smoke container absence is unproved");
    }
    Ok(())
}
pub fn api(image: &str) -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let name = format!("numera-api-smoke-{}-{nonce}", std::process::id());
    let token = format!("acceptance-{nonce}");
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let result = (|| -> Result<()> {
        let started = capture(
            Command::new("docker").args([
                "run",
                "-d",
                "--name",
                &name,
                "--env",
                &format!("NUMERA_API_TOKEN={token}"),
                "-p",
                &format!("127.0.0.1:{port}:8787"),
                image,
            ]),
            Duration::from_secs(30),
            Duration::from_secs(15),
            65536,
        )?;
        if started.exit != 0 {
            return fail("Native API container could not start");
        }
        let end = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            if crate::acceptance::request(port, "GET", "/api/health", Some(&token), None)
                .is_ok_and(|response| response.status == 200)
            {
                break;
            }
            if std::time::Instant::now() >= end {
                return fail("Native container did not become healthy within its deadline");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if crate::acceptance::request(port, "GET", "/api/health", None, None)?.status != 401 {
            return fail("Native container accepted an unauthenticated request");
        }
        let created = crate::acceptance::request(
            port,
            "POST",
            "/api/profiles",
            Some(&token),
            Some(&serde_json::json!({"name":"ci-smoke"})),
        )?;
        if created.status != 200 {
            return fail("Native container profile creation failed");
        }
        let profiles =
            crate::acceptance::request(port, "GET", "/api/profiles", Some(&token), None)?;
        if profiles.status != 200
            || !profiles
                .body
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["name"] == "ci-smoke"))
        {
            return fail("Native container did not retain its created profile");
        }
        Ok(())
    })();
    let _ = capture(
        Command::new("docker").args(["rm", "-f", &name]),
        Duration::from_secs(15),
        Duration::from_secs(10),
        65536,
    );
    let absent = capture(
        Command::new("docker").args([
            "ps",
            "-a",
            "--format",
            "{{.Names}}",
            "--filter",
            &format!("name=^/{name}$"),
        ]),
        Duration::from_secs(15),
        Duration::from_secs(10),
        65536,
    )?;
    if absent.exit != 0 || !absent.stdout.is_empty() {
        return fail("Owned API container absence is unproved");
    }
    result
}
pub fn definition(root: &Path) -> Result<()> {
    let text = std::fs::read_to_string(root.join("Dockerfile"))?;
    let compose = workflow::parse(&std::fs::read_to_string(root.join("docker-compose.yml"))?)?;
    if !text.contains("FROM ubuntu:24.04")
        || !text.contains("--verify-installation")
        || !text.contains("USER numera")
        || !text.contains("--http-healthcheck")
        || !text.contains("numera-browser-runtime")
        || !text.contains("build/container/Numera Browser/engine/")
    {
        return fail("Native container installation or execution contract differs");
    }
    let service = &compose["services"]["numera"];
    if service["ports"].as_array().is_none_or(|ports| {
        ports.is_empty()
            || ports.iter().any(|port| {
                port.as_str()
                    .is_none_or(|port| !port.starts_with("127.0.0.1:"))
            })
    }) || service["shm_size"] != "1gb"
        || service["environment"]["NUMERA_API_TOKEN"]
            .as_str()
            .is_none_or(|token| !token.starts_with("${NUMERA_API_TOKEN:?"))
    {
        return fail("Container bind,token or shared memory contract differs");
    }
    Ok(())
}
pub fn contract(workflows: &serde_json::Value) -> Result<()> {
    let ci = &workflows["native-ci"];
    workflow::native_automatic(ci)?;
    if ci["jobs"]["docker"]
        != serde_json::json!({"needs":"coverage","uses":"./.github/workflows/container.yml"})
    {
        return fail("Container verification must remain an independently required workflow");
    }
    let steps = &workflows["container"]["jobs"]["docker"]["steps"];
    workflow::step_once(steps, "ci container-browser", true)?;
    Ok(())
}
pub fn copy_sources(text: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut sources = Vec::new();
    for line in text.lines().filter_map(|line| line.strip_prefix("COPY ")) {
        if line.starts_with("--from") {
            continue;
        }
        let fields: Vec<String> = serde_json::from_str(line)?;
        if fields.len() < 2 {
            return fail("Container COPY declaration is incomplete");
        }
        for source in &fields[..fields.len() - 1] {
            super::release::public_path(source.trim_end_matches('/'), false)?;
            sources.push(source.into());
        }
    }
    if sources.is_empty() {
        return fail("Container has no exact product COPY source");
    }
    Ok(sources)
}
pub fn context(root: &Path, text: &str) -> Result<()> {
    for source in copy_sources(text)? {
        super::regular(&root.join(source), true)?;
    }
    Ok(())
}
pub fn prepare_live(
    root: &Path,
    worker: &Path,
    engine: &Path,
    resources: &Path,
    backend: &str,
    data: &Path,
    environment: &Path,
) -> Result<()> {
    use std::io::Write;
    if !["playwright", "patchright", "camoufox", "cloak"].contains(&backend) {
        return fail("Required live backend is unknown");
    }
    let identity = crate::artifact_identity::BuildIdentity {
        source_sha: "0".repeat(40),
        generation: 1,
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    };
    crate::resources::Receipt::validate(resources, &identity)
        .map_err(|_| "Required live resource receipt failed integrity verification")?;
    let manifest = super::decode(&std::fs::read(resources.join("browsers/manifest.json"))?)?;
    let mut values = std::collections::BTreeMap::from([
        ("NUMERA_NATIVE_CANDIDATE", engine.canonicalize()?),
        ("NUMERA_NATIVE_ENGINE", engine.canonicalize()?),
        ("NUMERA_BROWSER_RUNTIME", worker.canonicalize()?),
        (
            "NUMERA_BROWSER_DRIVERS",
            resources.join("drivers").canonicalize()?,
        ),
    ]);
    let binary = if backend == "cloak" {
        let acquisition = numera_core::cloak_acquisition::Acquisition::native_at(data);
        let offer = acquisition.offer();
        if offer["available"] != true {
            return fail(
                "Required Cloak backend needs an existing license key and the exact approved CLOAKBROWSER_VERSION",
            );
        }
        let id = offer["offer_id"]
            .as_str()
            .ok_or("Exact Cloak acquisition offer missing")?;
        let mut command = Command::new(engine);
        command.arg("--data-dir").arg(data).args([
            "engines",
            "--acquire-cloak",
            id,
            "--approve",
            "--json",
        ]);
        let output = owned::run(
            root,
            &mut command,
            owned::limits(
                Duration::from_secs(900),
                Duration::from_secs(120),
                1024 * 1024,
            ),
        )?
        .command;
        if output.exit != 0 || super::decode(&output.stdout)?["installed"] != true {
            return fail("Required exact-offer Cloak acquisition failed");
        }
        let selected = numera_core::cloak_acquisition::selected_binary(data)?;
        let path = selected
            .environment
            .get("CLOAKBROWSER_BINARY_PATH")
            .ok_or("Acquired Cloak binary path missing")?;
        std::path::PathBuf::from(path)
    } else {
        let relative = manifest[backend]["executable"]
            .as_str()
            .ok_or("Required live browser manifest is missing")?;
        super::release::public_path(relative, true)?;
        let base = resources.join("browsers").canonicalize()?;
        let selected = base.join(relative).canonicalize()?;
        if !selected.starts_with(base) || !selected.is_file() {
            return fail("Required live browser is outside its owned resource root");
        }
        selected
    };
    if backend == "camoufox" {
        #[cfg(target_os = "linux")]
        numera_core::browser::camoufox_fonts::prepare(&binary)?;
        values.insert("CAMOUFOX_EXECUTABLE_PATH", binary);
    } else if backend == "cloak" {
        values.insert("CLOAKBROWSER_BINARY_PATH", binary);
    } else {
        values.insert(
            "PLAYWRIGHT_BROWSERS_PATH",
            resources.join("browsers").join(backend).canonicalize()?,
        );
    }
    let mut output = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(environment)?;
    for (key, value) in values {
        let value = value
            .to_str()
            .ok_or("Live environment path is not Unicode")?;
        if value.contains(['\r', '\n']) {
            return fail("Live environment path contains a line break");
        }
        writeln!(output, "{key}={value}")?;
    }
    writeln!(output, "NUMERA_NATIVE_LIVE_ENGINE={backend}")?;
    output.sync_all()?;
    Ok(())
}
