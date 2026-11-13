//! Native acceptance commands share bounded process, HTTP and retained-evidence
//! boundaries with the source regression tests. No interpreter is invoked.
use crate::ci::{self, Result, fail};
use numera_core::ownership::{capture::Pipe, tree::OwnedTree, unit_report};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutePhase {
    Spawn,
    Record,
    Communicate,
    Decode,
    Settle,
    Wait,
    Close,
}
pub struct Captured {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}
/// Observers may fail closed (for example a durable receipt sink failing). Even
/// then the original tree is settled and every pipe closed before return.
pub fn execute_observed(
    command: &mut Command,
    input: Option<&[u8]>,
    parent: Option<&Path>,
    mut observe: impl FnMut(ExecutePhase, &Value) -> io::Result<()>,
) -> Result<Captured> {
    if input.is_some_and(|v| v.len() > 4096) {
        return fail("Acceptance stdin exceeds its atomic pipe bound");
    }
    observe(ExecutePhase::Spawn, &Value::Null)?;
    let attempt = parent.map(unit_report::begin_at).transpose()?;
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = OwnedTree::spawn(command)?;
    let record = tree.record();
    let mut pipes = [None, None];
    let result = (|| -> Result<Captured> {
        observe(ExecutePhase::Record, &record)?;
        if let (Some(parent), Some(attempt)) = (parent, attempt.as_deref()) {
            unit_report::record_at(parent, &record, attempt)?;
        }
        pipes = [
            Some(Pipe::Out(
                tree.child_mut()
                    .stdout
                    .take()
                    .ok_or("Captured stdout missing")?,
            )),
            Some(Pipe::Err(
                tree.child_mut()
                    .stderr
                    .take()
                    .ok_or("Captured stderr missing")?,
            )),
        ];
        for pipe in pipes.iter().flatten() {
            pipe.prepare()?;
        }
        observe(ExecutePhase::Communicate, &record)?;
        if let Some(input) = input {
            let mut stdin = tree
                .child_mut()
                .stdin
                .take()
                .ok_or("Captured stdin missing")?;
            stdin.write_all(input)?;
            drop(stdin);
        }
        let mut output = [Vec::new(), Vec::new()];
        let end = Instant::now() + Duration::from_secs(15);
        let mut buf = [0; 8192];
        let exit = loop {
            let status = tree.child_mut().try_wait()?;
            if pipes.iter().all(Option::is_none)
                && let Some(status) = status
            {
                break status.code().unwrap_or(-1);
            }
            if Instant::now() >= end {
                return fail("Acceptance command exceeded its communication budget");
            }
            let mut progress = false;
            for (index, slot) in pipes.iter_mut().enumerate() {
                let Some(pipe) = slot.as_mut() else { continue };
                match pipe.available(&mut buf)? {
                    Some(0) => {
                        *slot = None;
                        progress = true;
                    }
                    Some(count) => {
                        if output[0].len() + output[1].len() + count > 1_048_576 {
                            return fail("Acceptance output exceeds its bound");
                        }
                        output[index].extend_from_slice(&buf[..count]);
                        progress = true;
                    }
                    None => {}
                }
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        observe(ExecutePhase::Decode, &record)?;
        Ok(Captured {
            exit,
            stdout: String::from_utf8(std::mem::take(&mut output[0]))?,
            stderr: String::from_utf8(std::mem::take(&mut output[1]))?,
        })
    })();
    let cleanup_observation = observe(ExecutePhase::Settle, &record);
    let settled = tree.settle(Duration::from_secs(4));
    let cleanup = (|| -> Result<()> {
        cleanup_observation?;
        settled?;
        observe(ExecutePhase::Wait, &record)?;
        let end = Instant::now() + Duration::from_secs(5);
        while tree.child_mut().try_wait()?.is_none() {
            if Instant::now() >= end {
                return fail("Acceptance command did not reap");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    })();
    drop(pipes);
    drop(tree.child_mut().stdin.take());
    drop(tree.child_mut().stdout.take());
    drop(tree.child_mut().stderr.take());
    let closed = observe(ExecutePhase::Close, &record);
    cleanup?;
    closed?;
    result
}
pub fn execute(command: &mut Command, input: Option<&[u8]>) -> Result<Captured> {
    execute_observed(command, input, None, |_, _| Ok(()))
}

fn finite(value: &Value) -> bool {
    match value {
        Value::Number(n) => {
            let raw = n.to_string();
            let digits = raw.strip_prefix('-').unwrap_or(&raw);
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                || n.as_f64().is_some_and(f64::is_finite)
        }
        Value::Array(a) => a.iter().all(finite),
        Value::Object(o) => o.values().all(finite),
        _ => true,
    }
}
pub fn require_equal(observed: &Value, expected: &Value, label: &str) -> Result<()> {
    if !finite(observed) || !finite(expected) {
        return fail("Replay contains a nonfinite number");
    }
    // JSON values preserve booleans, integer/float kinds, complete key sets and
    // array order. Object storage order is immaterial.
    if observed != expected {
        return fail(format!(
            "Cross-runtime fixture differs: {}",
            if label.len() <= 64
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " -_".contains(c))
            {
                label
            } else {
                "fixture"
            }
        ));
    }
    Ok(())
}
pub fn require_http_profile(
    observed: &Value,
    expected: &Value,
    started: f64,
    finished: f64,
) -> Result<()> {
    for key in ["created_at", "updated_at"] {
        let n = observed[key]
            .as_f64()
            .ok_or("HTTP replay derived timestamp is invalid")?;
        if !n.is_finite()
            || !started.is_finite()
            || !finished.is_finite()
            || n < started
            || n > finished
        {
            return fail("HTTP replay derived timestamp is invalid");
        }
    }
    let stable = |v: &Value| -> Result<Value> {
        let mut v = v
            .as_object()
            .ok_or("HTTP replay profile is not an object")?
            .clone();
        v.remove("created_at");
        v.remove("updated_at");
        Ok(Value::Object(v))
    };
    require_equal(&stable(observed)?, &stable(expected)?, "HTTP profile")
}

pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, Value>,
    pub body: Value,
}
struct DeadlineReader {
    stream: TcpStream,
    end: Instant,
}
impl Read for DeadlineReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let remaining = self.end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP acceptance response exceeded its budget",
            ));
        }
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream.read(bytes)
    }
}
fn line(reader: &mut impl BufRead, remaining: &mut usize) -> io::Result<Vec<u8>> {
    let mut row = Vec::new();
    let read = reader
        .take((*remaining as u64) + 1)
        .read_until(b'\n', &mut row)?;
    if read == 0 || read > *remaining || !row.ends_with(b"\r\n") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid HTTP acceptance framing",
        ));
    }
    *remaining -= read;
    row.truncate(row.len() - 2);
    Ok(row)
}
pub fn request(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<Response> {
    let end = Instant::now() + Duration::from_secs(2);
    if !["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"].contains(&method)
        || !path.starts_with('/')
        || path.chars().any(|c| c.is_control() || c == ' ')
        || token.is_some_and(|s| s.chars().any(char::is_control))
    {
        return fail("Invalid HTTP acceptance request metadata");
    }
    let payload = body
        .map(|value| {
            if !finite(value) {
                return fail("HTTP acceptance body contains nonfinite values");
            }
            Ok(serde_json::to_vec(value)?)
        })
        .transpose()?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut headers =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    if let Some(token) = token {
        headers.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(payload) = &payload {
        headers.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        ));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    if let Some(payload) = payload {
        stream.write_all(&payload)?;
    }
    let mut reader = io::BufReader::new(DeadlineReader { stream, end });
    let mut bound = 16384;
    let status = line(&mut reader, &mut bound)?;
    let status = std::str::from_utf8(&status)?
        .split(' ')
        .nth(1)
        .ok_or("HTTP status missing")?
        .parse::<u16>()?;
    let mut all = BTreeMap::<String, String>::new();
    loop {
        let row = line(&mut reader, &mut bound)?;
        if row.is_empty() {
            break;
        }
        let row = std::str::from_utf8(&row)?;
        let (key, value) = row.split_once(':').ok_or("HTTP response header invalid")?;
        let key = key.to_ascii_lowercase();
        let framing = matches!(key.as_str(), "content-length" | "transfer-encoding");
        if all.insert(key, value.trim().into()).is_some() && framing {
            return fail("Duplicate HTTP acceptance header");
        }
    }
    let mut bytes = Vec::new();
    if method != "HEAD" && ![204, 304].contains(&status) && !(100..200).contains(&status) {
        if let Some(encoding) = all.get("transfer-encoding") {
            if !encoding.eq_ignore_ascii_case("chunked") || all.contains_key("content-length") {
                return fail("HTTP acceptance transfer framing is ambiguous");
            }
            loop {
                let row = line(&mut reader, &mut bound)?;
                let n = std::str::from_utf8(&row)?
                    .split(';')
                    .next()
                    .ok_or("Chunk size missing")?;
                let n = usize::from_str_radix(n, 16)?;
                if n > 65536usize.saturating_sub(bytes.len()) {
                    return fail("HTTP acceptance response exceeds bound");
                }
                if n == 0 {
                    while !line(&mut reader, &mut bound)?.is_empty() {}
                    break;
                }
                let before = bytes.len();
                bytes.resize(before + n, 0);
                reader.read_exact(&mut bytes[before..])?;
                let mut crlf = [0; 2];
                reader.read_exact(&mut crlf)?;
                if crlf != *b"\r\n" {
                    return fail("HTTP acceptance chunk terminator invalid");
                }
            }
        } else if let Some(length) = all.get("content-length") {
            let n = length.parse::<usize>()?;
            if n > 65536 {
                return fail("HTTP acceptance response exceeds bound");
            }
            bytes.resize(n, 0);
            reader.read_exact(&mut bytes)?;
        } else {
            reader.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return fail("HTTP acceptance response exceeds bound");
            }
        }
    }
    let mut selected = BTreeMap::new();
    for (key, value) in &all {
        if [
            "content-type",
            "access-control-allow-origin",
            "www-authenticate",
        ]
        .contains(&key.as_str())
            || (["/openapi.json", "/docs", "/docs/oauth2-redirect", "/redoc"].contains(&path)
                && ["allow", "content-length"].contains(&key.as_str()))
        {
            selected.insert(
                key.clone(),
                if key == "allow" {
                    let mut methods = value.split(',').map(str::trim).collect::<Vec<_>>();
                    methods.sort();
                    json!(methods)
                } else {
                    json!(value)
                },
            );
        }
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else if all
        .get("content-type")
        .is_some_and(|v| v.starts_with("application/json"))
    {
        ci::decode(&bytes)?
    } else {
        Value::String(String::from_utf8(bytes)?)
    };
    Ok(Response {
        status,
        headers: selected,
        body,
    })
}

/// Only a directory created exclusively by this owner is ever reclaimed. A
/// failed run deliberately has no automatic deletion path.
pub struct Fixture {
    path: PathBuf,
}
impl Fixture {
    pub fn create(path: &Path) -> Result<Self> {
        fs::create_dir(path)?;
        Ok(Self {
            path: path.to_owned(),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn finish<T>(self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => {
                fs::remove_dir_all(&self.path)?;
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }
}
pub fn sdk_units_with(
    root: &Path,
    job: &Value,
    reference: &str,
    mut alive: impl FnMut(&Value) -> io::Result<bool>,
) -> Result<Vec<Value>> {
    numera_core::storage::paths::safe_id(reference).map_err(io::Error::other)?;
    let id = job["id"]
        .as_str()
        .ok_or("Detached ownership job ID missing")?;
    numera_core::storage::paths::safe_id(id).map_err(io::Error::other)?;
    let report = unit_report::read(&root.join("jobs").join(format!("{id}.units")))?;
    if !report.complete {
        return fail("Detached ownership receipt is incomplete or malformed");
    }
    let command = job
        .as_object()
        .ok_or("Detached ownership command invalid")?;
    let matches = |tree: &Value, record: &serde_json::Map<String, Value>| {
        tree["pid"] == record.get("pid").cloned().unwrap_or(Value::Null)
            && tree["pidToken"] == record.get("pidToken").cloned().unwrap_or(Value::Null)
            && tree["tree"] == record.get("tree").cloned().unwrap_or(Value::Null)
    };
    if !report.trees.iter().any(|tree| matches(tree, command)) {
        return fail("Detached command ownership identity was not recorded");
    }
    let claim: Value = ci::decode(&fs::read(
        root.join("sessions").join(format!("{reference}.json")),
    )?)?;
    let mut sdk = claim
        .as_object()
        .ok_or("SDK session claim invalid")?
        .clone();
    sdk.insert("pidToken".into(), claim["token"].clone());
    let sdk = Value::Object(sdk);
    if sdk["pid"] == job["pid"]
        || !report
            .trees
            .iter()
            .any(|tree| matches(tree, sdk.as_object().expect("constructed object")))
        || !alive(&sdk)?
    {
        return fail("Session SDK ownership identity was not recorded separately or is not live");
    }
    Ok(report.trees)
}
pub fn sdk_units(root: &Path, job: &Value, reference: &str) -> Result<Vec<Value>> {
    sdk_units_with(root, job, reference, |record| {
        numera_core::ownership::recorded_tree::empty(record).map(|empty| !empty)
    })
}

fn candidate_command(candidate: &Path, root: &Path) -> Command {
    let mut c = Command::new(candidate);
    c.arg("--data-dir").arg(root);
    for key in [
        "NUMERA_PASSWORD",
        "NUMERA_API_TOKEN",
        "NUMERA_DATA_DIR",
        "NUMERA_UPDATE_TOKEN",
        "NUMERA_UPDATE_TICKET",
        "NUMERA_UPDATE_ACTIVATION",
        "NUMERA_NATIVE_SUPERVISE",
        "NUMERA_NATIVE_JOB_GATE",
        "NUMERA_HELPER_UNITS",
        "NUMERA_HELPER_SCOPE",
        "NUMERA_PARENT_PID",
    ] {
        c.env_remove(key);
    }
    c.env("NUMERA_LOG_DIR", root.join("logs"));
    c
}
pub fn with_server<T>(
    candidate: &Path,
    root: &Path,
    token: &str,
    exercise: impl FnOnce(u16) -> Result<T>,
) -> Result<T> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut command = candidate_command(candidate, root);
    command
        .args(["serve", "--host", "127.0.0.1", "--port", &port.to_string()])
        .env("NUMERA_API_TOKEN", token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut tree = OwnedTree::spawn(&mut command)?;
    let result = (|| {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if request(port, "GET", "/api/ready", None, None).is_ok_and(|r| r.status == 200) {
                break;
            }
            if tree.child_mut().try_wait()?.is_some() {
                return fail("HTTP acceptance server exited before readiness");
            }
            if Instant::now() >= end {
                return fail("HTTP acceptance server did not become ready");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        exercise(port)
    })();
    tree.settle(Duration::from_secs(4))?;
    if TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(200),
    )
    .is_ok()
    {
        return fail("HTTP listener survived owned cleanup");
    }
    result
}
pub fn run(candidate: &Path, path: &Path) -> Result<Value> {
    ci::regular(candidate, false)?;
    let fixture = Fixture::create(path)?;
    let result = (|| {
        let guard = numera_core::ownership::resources::ResourceGuard::new()?;
        let root = fixture.path().join("data");
        fs::create_dir(&root)?;
        let created = execute(
            candidate_command(candidate, &root).args(["create", "Acceptance α 日本語", "--json"]),
            None,
        )?;
        if created.exit != 0 || !created.stderr.is_empty() {
            return fail("Native acceptance create command failed");
        }
        let created = ci::decode(created.stdout.as_bytes())?;
        let created = created
            .get("profile")
            .ok_or("Native acceptance create profile missing")?;
        let id = created["id"]
            .as_str()
            .ok_or("Native acceptance create ID missing")?;
        let shown = execute(
            candidate_command(candidate, &root).args(["show", id, "--json"]),
            None,
        )?;
        if shown.exit != 0 {
            return fail("Native acceptance show failed");
        }
        require_equal(
            &ci::decode(shown.stdout.as_bytes())?,
            created,
            "create show",
        )?;
        with_server(candidate, &root, "acceptance-only", |port| {
            let unauthorized = request(port, "GET", "/api/profiles", None, None)?;
            if unauthorized.status != 401 {
                return fail("Native HTTP acceptance failed closed authentication");
            }
            let profiles = request(port, "GET", "/api/profiles", Some("acceptance-only"), None)?;
            if profiles.status != 200
                || profiles
                    .body
                    .as_array()
                    .is_none_or(|p| p.len() != 1 || p[0]["id"] != id)
            {
                return fail("Native HTTP acceptance lost shared-store identity");
            }
            let notes = json!({"notes":"Fixture α 日本語","_rev":profiles.body[0]["_rev"]});
            let changed = request(
                port,
                "PUT",
                &format!("/api/profiles/{id}"),
                Some("acceptance-only"),
                Some(&notes),
            )?;
            if changed.status != 200 || changed.body["notes"] != notes["notes"] {
                return fail("Native HTTP UTF8 replay failed");
            }
            Ok(())
        })?;
        let listed = execute(
            candidate_command(candidate, &root).args(["show", id, "--json"]),
            None,
        )?;
        if listed.exit != 0 || ci::decode(listed.stdout.as_bytes())?["notes"] != "Fixture α 日本語"
        {
            return fail("Native CLI did not observe HTTP replay");
        }
        let resources = guard.finish()?;
        Ok(
            json!({"schema":1,"native":true,"cli":true,"http":true,"shared_store":true,"resources":resources}),
        )
    })();
    fixture.finish(result)
}
pub fn command(args: &[std::ffi::OsString]) -> Result<()> {
    match args.first().and_then(|s| s.to_str()) {
        Some("owned-run") if args.len() >= 6 && args[4] == "--" => {
            let wall = args[2].to_string_lossy().parse::<f64>()?;
            let idle = args[3].to_string_lossy().parse::<f64>()?;
            if !wall.is_finite() || !idle.is_finite() || wall <= 0.0 || idle <= 0.0 {
                return fail("Owned acceptance requires positive finite wall/idle limits");
            }
            let wall = Duration::try_from_secs_f64(wall)?;
            let idle = Duration::try_from_secs_f64(idle)?;
            let fixture = Fixture::create(Path::new(&args[1]))?;
            let guard = numera_core::ownership::resources::ResourceGuard::new()?;
            let mut child = Command::new(&args[5]);
            child.args(&args[6..]);
            for key in ["TMPDIR", "TEMP", "TMP"] {
                child.env(key, fixture.path());
            }
            let started = Instant::now();
            let output = ci::owned::run(
                fixture.path(),
                &mut child,
                ci::owned::limits(wall, idle, 8 * 1024 * 1024),
            );
            let resources = guard.finish();
            if output.as_ref().is_err_and(|error| {
                error
                    .downcast_ref::<numera_core::ownership::capture::Failure>()
                    .is_some_and(|e| e.kind() == io::ErrorKind::TimedOut)
            }) {
                return finish_timeout(
                    fixture,
                    TimeoutObservation {
                        elapsed: started.elapsed(),
                        silence: idle,
                        wall,
                        idle,
                    },
                    resources,
                    |line| eprintln!("{line}"),
                );
            }
            resources?;
            remove_owned_with(
                fixture.path(),
                |_, _| Ok(()),
                |diagnostic| eprintln!("{diagnostic}"),
            )?;
            let output = output?;
            io::stdout().write_all(&output.command.stdout)?;
            io::stderr().write_all(&output.command.stderr)?;
            if output.command.exit != 0 {
                return fail("Owned acceptance command failed");
            }
            return Ok(());
        }
        Some("phase") if args.len() == 2 => {
            return native_phase(&ci::root(), &args[1].to_string_lossy());
        }
        Some("export-native") if args.len() == 4 => {
            return export_native(
                &args[1].to_string_lossy(),
                Path::new(&args[2]),
                Some(Path::new(&args[3])),
            );
        }
        Some("report") if args.len() == 3 => {
            let expected = ci::decode(&fs::read(&args[1])?)?
                .as_array()
                .ok_or("Required native case inventory is not an array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("Required native case is not text")
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut report = ci::evidence::Current::new(expected)?;
            for line in std::str::from_utf8(&fs::read(&args[2])?)?.lines() {
                report.accept(line);
            }
            return report.verify(0);
        }
        Some("module-report") if args.len() == 2 => {
            let baseline = ci::decode(&fs::read(ci::root().join("docs/rust-test-baseline.json"))?)?;
            let mut expected = BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            for id in baseline["source_test_cases"]["python"]
                .as_array()
                .ok_or("Frozen source inventory missing")?
            {
                let id = id.as_str().ok_or("Source ID is not text")?;
                let (module, case) = id.split_once("::").ok_or("Source ID is unqualified")?;
                let module = Path::new(module)
                    .file_stem()
                    .and_then(|p| p.to_str())
                    .ok_or("Source module is not text")?;
                expected
                    .entry(module.into())
                    .or_default()
                    .insert(case.into());
            }
            let report = ci::decode(&fs::read(&args[1])?)?;
            println!(
                "{}",
                verify_modules(
                    &expected,
                    &report["results"],
                    report["platform"]
                        .as_str()
                        .ok_or("Report platform missing")?
                )?
            );
            return Ok(());
        }
        Some("audit-lock") if args.len() == 3 => {
            let locals = args[2].to_string_lossy();
            let locals = locals.split(',').collect::<Vec<_>>();
            let packages = locked_dependencies(&ci::decode(&fs::read(&args[1])?)?, &locals)?;
            println!(
                "Verified {} pinned third-party package records",
                packages.len()
            );
            return Ok(());
        }
        _ => {}
    }
    if args.len() != 2 {
        return fail(
            "acceptance requires candidate executable and a new retained-fixture directory",
        );
    }
    println!("{}", run(Path::new(&args[0]), Path::new(&args[1]))?);
    Ok(())
}

/// Hosted handoff is append-only and only the consent-selected native Cloak
/// executable is exported. The caller supplies the environment file explicitly.
pub fn export_native(engine: &str, binary: &Path, environment: Option<&Path>) -> Result<()> {
    if engine != "cloak" {
        return Ok(());
    }
    let path = binary.canonicalize()?;
    ci::regular(&path, false)?;
    let value = path.to_str().ok_or("Native browser path is not Unicode")?;
    export_value(value, environment)
}
pub fn export_value(value: &str, environment: Option<&Path>) -> Result<()> {
    if value.chars().any(char::is_control) {
        return fail("Native path has invalid environment characters");
    }
    let environment = environment.ok_or("GitHub environment file is required")?;
    let mut output = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(environment)?;
    writeln!(output, "CLOAKBROWSER_BINARY_PATH={value}")?;
    output.sync_all()?;
    Ok(())
}
pub fn reject_ignored_library(paths: &[PathBuf]) -> Result<()> {
    let attribute = regex::Regex::new(r#"#\s*\[\s*ignore(?:\s*=\s*"[^"]*")?\s*\]"#)?;
    for path in paths {
        if attribute.is_match(&fs::read_to_string(path)?) {
            return fail("Native library has an unmapped ignored test");
        }
    }
    Ok(())
}
pub fn library_sources(root: &Path) -> Result<Vec<PathBuf>> {
    fn visit(path: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return fail("Native library source is linked");
            }
            if metadata.is_dir() {
                visit(&path, output)?;
            } else if path.extension().is_some_and(|s| s == "rs") {
                output.push(path);
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    visit(&root.join("engine-rust/src"), &mut output)?;
    Ok(output)
}
pub fn locked_dependencies(metadata: &Value, local: &[&str]) -> Result<Vec<Value>> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("Dependency package inventory missing")?;
    let mut found = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    let version = regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?$")?;
    for package in packages {
        let name = package["name"].as_str().ok_or("Dependency name missing")?;
        let source = package["source"].as_str();
        if local.contains(&name) && source.is_none() {
            if !found.insert(name.to_owned()) {
                return fail("Local package is duplicated");
            }
            continue;
        }
        if source.is_none_or(|s| !s.starts_with("registry+") && !s.starts_with("git+"))
            || package["version"]
                .as_str()
                .is_none_or(|v| !version.is_match(v))
        {
            return fail("Dependency is not a pinned third-party package");
        }
        result.push(package.clone());
    }
    if found != local.iter().map(|s| (*s).to_owned()).collect() {
        return fail("Exact local distribution inventory is missing");
    }
    Ok(result)
}
pub trait NativePhases {
    fn fetch(&mut self) -> Result<()>;
    fn history(&mut self) -> Result<()>;
    fn build(&mut self) -> Result<()>;
    fn test(&mut self) -> Result<()>;
}
pub fn run_phase(phase: &str, runner: &mut impl NativePhases) -> Result<()> {
    match phase {
        "history" => {
            runner.fetch()?;
            runner.history()
        }
        "build" => runner.build(),
        "test" => runner.test(),
        _ => fail("Native phase must be history,build or test"),
    }
}
struct Native<'a>(&'a Path);
impl NativePhases for Native<'_> {
    fn fetch(&mut self) -> Result<()> {
        let output = ci::capture(
            Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["fetch", "--manifest-path"])
                .arg(self.0.join("engine-rust/Cargo.toml"))
                .arg("--locked"),
            Duration::from_secs(300),
            Duration::from_secs(180),
            1_048_576,
        )?;
        if output.exit != 0 {
            return fail("Native dependency fetch failed");
        }
        Ok(())
    }
    fn history(&mut self) -> Result<()> {
        ci::history::native(self.0)
    }
    fn build(&mut self) -> Result<()> {
        reject_ignored_library(&library_sources(self.0)?)?;
        ci::scope::native_phase(self.0, "build")
    }
    fn test(&mut self) -> Result<()> {
        ci::scope::native_phase(self.0, "test")
    }
}
pub fn native_phase(root: &Path, phase: &str) -> Result<()> {
    run_phase(phase, &mut Native(root))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupOperation {
    Unlink,
    Rmdir,
    Scandir,
    Islink,
    Unknown,
}
impl CleanupOperation {
    pub fn from_name(name: &str) -> Self {
        match name {
            "unlink" => Self::Unlink,
            "rmdir" => Self::Rmdir,
            "scandir" => Self::Scandir,
            "islink" => Self::Islink,
            _ => Self::Unknown,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Unlink => "unlink",
            Self::Rmdir => "rmdir",
            Self::Scandir => "scandir",
            Self::Islink => "islink",
            Self::Unknown => "unknown",
        }
    }
}
pub struct CleanupFault {
    pub operation: CleanupOperation,
    pub error: io::Error,
    pub winerror: Option<u32>,
}
#[derive(Debug)]
pub struct CleanupDiagnostic {
    pub operation: CleanupOperation,
    pub errno: Option<i32>,
    pub winerror: Option<u32>,
    pub kind: &'static str,
    pub readonly: bool,
    pub reparse: bool,
}
impl std::fmt::Display for CleanupDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Owned temp cleanup failed operation={} errno={} winerror={} kind={} readonly={} reparse={}",
            self.operation.name(),
            self.errno.map_or_else(|| "None".into(), |n| n.to_string()),
            self.winerror
                .map_or_else(|| "None".into(), |n| n.to_string()),
            self.kind,
            self.readonly,
            self.reparse
        )
    }
}
fn cleanup_fault(operation: CleanupOperation, error: io::Error) -> CleanupFault {
    #[cfg(windows)]
    let winerror = error.raw_os_error().and_then(|n| u32::try_from(n).ok());
    #[cfg(not(windows))]
    let winerror = None;
    CleanupFault {
        operation,
        error,
        winerror,
    }
}
/// Filesystem operations are observable without exposing paths or OS exception
/// strings in diagnostics. The first failure is returned exactly once.
pub fn remove_owned_with(
    path: &Path,
    mut before: impl FnMut(CleanupOperation, &Path) -> std::result::Result<(), CleanupFault>,
    mut emit: impl FnMut(CleanupDiagnostic),
) -> io::Result<()> {
    fn remove(
        path: &Path,
        before: &mut impl FnMut(CleanupOperation, &Path) -> std::result::Result<(), CleanupFault>,
    ) -> std::result::Result<(), (CleanupFault, Option<Box<fs::Metadata>>)> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|e| (cleanup_fault(CleanupOperation::Islink, e), None))?;
        before(CleanupOperation::Islink, path)
            .map_err(|e| (e, Some(Box::new(metadata.clone()))))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            before(CleanupOperation::Scandir, path)
                .map_err(|e| (e, Some(Box::new(metadata.clone()))))?;
            let entries = fs::read_dir(path).map_err(|e| {
                (
                    cleanup_fault(CleanupOperation::Scandir, e),
                    Some(Box::new(metadata.clone())),
                )
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| {
                    (
                        cleanup_fault(CleanupOperation::Scandir, e),
                        Some(Box::new(metadata.clone())),
                    )
                })?;
                remove(&entry.path(), before)?;
            }
            before(CleanupOperation::Rmdir, path)
                .map_err(|e| (e, Some(Box::new(metadata.clone()))))?;
            Ok(())
        } else {
            before(CleanupOperation::Unlink, path)
                .map_err(|e| (e, Some(Box::new(metadata.clone()))))?;
            Ok(())
        }
    }
    let result = remove(path, &mut before).and_then(|()| {
        let metadata = fs::symlink_metadata(path)
            .map_err(|e| (cleanup_fault(CleanupOperation::Islink, e), None))?;
        let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        result.map_err(|e| {
            (
                cleanup_fault(CleanupOperation::Unknown, e),
                Some(Box::new(metadata)),
            )
        })
    });
    match result {
        Ok(()) => Ok(()),
        Err((fault, metadata)) => {
            let kind = metadata.as_ref().map_or("unknown", |m| {
                if m.is_file() {
                    "file"
                } else if m.is_dir() {
                    "directory"
                } else if m.file_type().is_symlink() {
                    "link"
                } else {
                    "other"
                }
            });
            let readonly = metadata
                .as_ref()
                .is_some_and(|m| m.permissions().readonly());
            #[cfg(windows)]
            let reparse = {
                use std::os::windows::fs::MetadataExt;
                metadata
                    .as_ref()
                    .is_some_and(|m| m.file_attributes() & 0x400 != 0)
            };
            #[cfg(not(windows))]
            let reparse = metadata
                .as_ref()
                .is_some_and(|m| m.file_type().is_symlink());
            emit(CleanupDiagnostic {
                operation: fault.operation,
                errno: fault.error.raw_os_error(),
                winerror: fault.winerror,
                kind,
                readonly,
                reparse,
            });
            Err(fault.error)
        }
    }
}
#[derive(Clone, Copy)]
pub struct TimeoutObservation {
    pub elapsed: Duration,
    pub silence: Duration,
    pub wall: Duration,
    pub idle: Duration,
}
impl TimeoutObservation {
    pub fn message(self) -> String {
        format!(
            "Verification timeout wall_expired={} idle_expired={} elapsed={:.3}s silence={:.3}s wall={:.3}s idle={:.3}s",
            self.elapsed >= self.wall,
            self.silence >= self.idle,
            self.elapsed.as_secs_f64(),
            self.silence.as_secs_f64(),
            self.wall.as_secs_f64(),
            self.idle.as_secs_f64()
        )
    }
}
/// A timeout remains the primary result only after tree/pipe/resource proof.
/// Uncertain resources retain the owned directory and the timeout is recorded
/// before cleanup diagnostics can supersede it.
pub fn finish_timeout(
    fixture: Fixture,
    timeout: TimeoutObservation,
    resources: std::result::Result<
        numera_core::ownership::resources::ResourceEvidence,
        numera_core::ownership::resources::ResourceFailure,
    >,
    mut emit: impl FnMut(String),
) -> Result<()> {
    emit(timeout.message());
    emit("Owned verification phase=settle-tree".into());
    resources?;
    remove_owned_with(
        fixture.path(),
        |_, _| Ok(()),
        |diagnostic| emit(diagnostic.to_string()),
    )?;
    fail("Migration command exceeded wall/idle budget")
}

/// Native per-source reports enumerate actual test identities. A whole-module
/// compile-time exclusion is accepted only for the two reviewed POSIX-only
/// source domains on Windows; mixed modules still require real passing cases.
pub fn verify_modules(
    expected: &BTreeMap<String, std::collections::BTreeSet<String>>,
    rows: &Value,
    platform: &str,
) -> Result<Value> {
    let rows = rows
        .as_array()
        .ok_or("Source module report is not an array")?;
    let mut seen = std::collections::BTreeSet::new();
    let mut passed = 0;
    let mut limits = 0;
    for row in rows {
        let module = row["module"].as_str().ok_or("Reported module missing")?;
        let wanted = expected
            .get(module)
            .ok_or("Unexpected verification report module")?;
        if !seen.insert(module.to_owned()) {
            return fail("Duplicate verification report module");
        }
        let cases = row["cases"].as_array().ok_or("Reported cases missing")?;
        let mut names = std::collections::BTreeSet::new();
        let mut module_passed = 0;
        let mut excluded = Vec::new();
        for case in cases {
            let id = case["id"].as_str().ok_or("Reported case ID missing")?;
            if !wanted.contains(id) || !names.insert(id.to_owned()) {
                return fail("Unknown or duplicate verification case");
            }
            match case["status"].as_str() {
                Some("passed") => {
                    passed += 1;
                    module_passed += 1;
                }
                Some("cfg-excluded") => {
                    excluded.push(
                        case["reason"]
                            .as_str()
                            .ok_or("Compile-time exclusion reason missing")?,
                    );
                    limits += 1;
                }
                _ => return fail("Verification case failed,skipped or did not execute"),
            }
        }
        if &names != wanted {
            return fail("Maintained source case omitted from report");
        }
        if module_passed == 0 {
            let allowed = match (platform, module) {
                ("win32", "test_posix_observation_errors") => {
                    Some("POSIX process-group observation")
                }
                ("win32", "test_shell_death_cleanup") => {
                    Some("Windows owns the engine through a job object")
                }
                _ => None,
            };
            if allowed.is_none_or(|reason| {
                excluded.is_empty() || excluded.iter().any(|s| !s.contains(reason))
            }) {
                return fail(
                    "Whole-module exclusion waived a mixed-platform or required source suite",
                );
            }
        } else if !excluded.is_empty() {
            return fail("Partial runtime omissions are not native passing evidence");
        }
    }
    if seen != expected.keys().cloned().collect() {
        return fail("Maintained source suite missing from report");
    }
    Ok(json!({"passed":passed,"explicit_limits":limits,"suites":seen.len()}))
}
