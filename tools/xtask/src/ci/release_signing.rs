//! Explicit manual release tools. No ambient owner key is read by validation.
use super::{Result, fail, regular};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const SECRETS: [&str; 3] = [
    "NUMERA_RELEASE_SIGNING_KEY",
    "NUMERA_CODE_SIGNING_PFX",
    "NUMERA_CODE_SIGNING_PASSWORD",
];
fn message(path: &Path) -> Result<Vec<u8>> {
    regular(path, false)?;
    let bytes = fs::read(path)?;
    let limit = if path.file_name().is_some_and(|s| s == "BUILD.json") {
        8 * 1024 * 1024
    } else {
        1024 * 1024
    };
    if bytes.is_empty() || bytes.len() > limit {
        return fail("Invalid release manifest size");
    }
    Ok(bytes)
}
pub fn verify_manifest(path: &Path, signature: &Path, key: &VerifyingKey) -> Result<()> {
    let bytes = message(path)?;
    regular(signature, false)?;
    let proof = fs::read(signature)?;
    if proof.len() != 64 {
        return fail("Invalid raw signature size");
    }
    let proof = Signature::from_slice(&proof).map_err(|_| "Invalid raw signature size")?;
    key.verify_strict(&bytes, &proof)
        .map_err(|_| "Release signature does not match pinned signer and exact manifest".into())
}
pub fn sign_manifest(
    path: &Path,
    signature: &Path,
    key: &SigningKey,
    pinned: &VerifyingKey,
) -> Result<()> {
    let bytes = message(path)?;
    if signature.exists() {
        return fail("Manifest signature destination new required");
    }
    if key.verifying_key() != *pinned {
        return fail("Protected release key differs from pinned public signer");
    }
    let proof = key.sign(&bytes);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(signature)?;
    output.write_all(&proof.to_bytes())?;
    output.sync_all()?;
    verify_manifest(path, signature, pinned)
}
fn pem(text: &[u8], label: &str, prefix: &[u8], length: usize) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(text)?;
    let body = text
        .strip_prefix(&format!("-----BEGIN {label}-----"))
        .and_then(|s| s.trim().strip_suffix(&format!("-----END {label}-----")))
        .ok_or("Release key PEM type differs")?;
    let mut decoded = STANDARD.decode(
        body.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
    )?;
    if decoded.len() != prefix.len() + length || !decoded.starts_with(prefix) {
        decoded.fill(0);
        return fail("Release key must be canonical Ed25519 PKCS8 PEM");
    }
    let value = decoded.split_off(prefix.len());
    decoded.fill(0);
    Ok(value)
}
pub fn public_key(path: &Path) -> Result<VerifyingKey> {
    let value = pem(
        &fs::read(path)?,
        "PUBLIC KEY",
        &[0x30, 0x2a, 0x30, 5, 6, 3, 0x2b, 0x65, 0x70, 3, 0x21, 0],
        32,
    )?;
    Ok(VerifyingKey::from_bytes(
        &value.try_into().map_err(|_| "Public key size differs")?,
    )?)
}
pub(super) fn private_key(text: &[u8]) -> Result<SigningKey> {
    let mut value = pem(
        text,
        "PRIVATE KEY",
        &[
            0x30, 0x2e, 2, 1, 0, 0x30, 5, 6, 3, 0x2b, 0x65, 0x70, 4, 0x22, 4, 0x20,
        ],
        32,
    )?;
    let key = SigningKey::from_bytes(
        value
            .as_slice()
            .try_into()
            .map_err(|_| "Private key size differs")?,
    );
    value.fill(0);
    Ok(key)
}
pub fn owner_context(env: &BTreeMap<String, String>) -> Result<()> {
    let get = |name: &str| env.get(name).map_or("", String::as_str);
    if get("GITHUB_ACTIONS") == "true"
        && (get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            || get("GITHUB_ACTOR") != "KiaroSama"
            || ![
                "refs/heads/main",
                "refs/heads/migration/native-runtime-checkpoint-20261012",
            ]
            .contains(&get("GITHUB_REF"))
            || get("NUMERA_TRUSTED_SHA") != get("GITHUB_SHA"))
    {
        return fail("Owner signing requires exact approved manual source revision");
    }
    Ok(())
}
pub fn tool_environment(env: &BTreeMap<String, String>, program: &str) -> BTreeMap<String, String> {
    let mut env = env.clone();
    for name in SECRETS {
        env.remove(name);
    }
    if program == "codesign" {
        env.insert("LC_ALL".into(), "C".into());
        env.insert("LANG".into(), "C".into());
    }
    env
}
#[derive(Clone)]
pub struct ToolRequest {
    pub program: String,
    pub args: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
    pub input: Vec<u8>,
    pub seconds: u64,
}
#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub exit: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub fn execute_with<F>(
    mut request: ToolRequest,
    expected: i32,
    diagnostic: Option<&[u8]>,
    tool: &mut F,
) -> Result<Vec<u8>>
where
    F: FnMut(&ToolRequest) -> io::Result<ToolOutput>,
{
    request.environment = tool_environment(&request.environment, &request.program);
    let result = tool(&request);
    request.input.fill(0);
    let output = match result {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::TimedOut => {
            return fail(format!(
                "Signing tool exceeded its deadline: {}",
                request.program
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                e.kind(),
                format!("Signing tool missing: {}", request.program),
            )
            .into());
        }
        Err(_) => return fail(format!("Signing tool failed: {}", request.program)),
    };
    if output.exit != expected
        || diagnostic.is_some_and(|part| {
            !output
                .stderr
                .windows(part.len())
                .any(|window| window == part)
        })
    {
        return fail(format!(
            "Signing tool failed: {} exit={}",
            request.program, output.exit
        ));
    }
    Ok(output.stdout)
}
fn pending_tools() -> &'static std::sync::Mutex<Vec<numera_core::ownership::tree::OwnedTree>> {
    static OWNERS: std::sync::OnceLock<
        std::sync::Mutex<Vec<numera_core::ownership::tree::OwnedTree>>,
    > = std::sync::OnceLock::new();
    OWNERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}
pub fn retry_tool_cleanup() -> io::Result<()> {
    let mut owners = pending_tools()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = std::mem::take(&mut *owners);
    let mut failure = None;
    for mut owner in previous {
        if let Err(error) = owner.settle(Duration::from_secs(1)) {
            owners.push(owner);
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
pub fn execute(request: &ToolRequest) -> io::Result<ToolOutput> {
    execute_bounded(request, 2 * 1024 * 1024)
}
/// The same retained process boundary with a narrower caller output budget.
pub fn execute_bounded(request: &ToolRequest, output_bytes: usize) -> io::Result<ToolOutput> {
    use numera_core::ownership::{capture::Pipe, tree::OwnedTree};
    use std::{
        process::{Command, Stdio},
        time::Instant,
    };
    retry_tool_cleanup()?;
    if request.input.len() > 4096
        || request.seconds == 0
        || output_bytes == 0
        || output_bytes > 2 * 1024 * 1024
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Signing input or deadline exceeds bound",
        ));
    }
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .env_clear()
        .envs(&request.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = OwnedTree::spawn(&mut command)?;
    let result = (|| -> io::Result<ToolOutput> {
        let mut pipes = [
            Some(Pipe::Out(
                tree.child_mut()
                    .stdout
                    .take()
                    .ok_or_else(|| io::Error::other("Signing stdout missing"))?,
            )),
            Some(Pipe::Err(
                tree.child_mut()
                    .stderr
                    .take()
                    .ok_or_else(|| io::Error::other("Signing stderr missing"))?,
            )),
        ];
        for pipe in pipes.iter().flatten() {
            pipe.prepare()?;
        }
        if let Some(mut input) = tree.child_mut().stdin.take() {
            input.write_all(&request.input)?;
        }
        let mut outputs = [Vec::new(), Vec::new()];
        let mut bytes = [0; 8192];
        let end = Instant::now() + Duration::from_secs(request.seconds);
        loop {
            let status = tree.child_mut().try_wait()?;
            if pipes.iter().all(Option::is_none)
                && let Some(status) = status
            {
                return Ok(ToolOutput {
                    exit: status.code().unwrap_or(-1),
                    stdout: std::mem::take(&mut outputs[0]),
                    stderr: std::mem::take(&mut outputs[1]),
                });
            }
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Signing deadline exceeded",
                ));
            }
            for (index, slot) in pipes.iter_mut().enumerate() {
                if let Some(pipe) = slot {
                    match pipe.available(&mut bytes)? {
                        Some(0) => *slot = None,
                        Some(count) => {
                            if outputs[0].len() + outputs[1].len() + count > output_bytes {
                                return Err(io::Error::other("Signing output exceeds bound"));
                            }
                            outputs[index].extend_from_slice(&bytes[..count]);
                        }
                        None => (),
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(3));
        }
    })();
    if let Err(error) = tree.settle(Duration::from_secs(10)) {
        pending_tools()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(tree);
        return Err(error);
    }
    result
}

struct Private(PathBuf);
impl Private {
    fn new(parent: &Path) -> Result<Self> {
        fs::create_dir_all(parent)?;
        let path = parent.join(format!(
            "numera-sign-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&path)?;
        let owned = Self(path.clone());
        #[cfg(windows)]
        {
            let environment = std::env::vars().collect();
            let name = execute_with(
                request("whoami", Vec::new(), &environment),
                0,
                None,
                &mut execute,
            )?;
            let name = String::from_utf8(name)?.trim().to_owned();
            if name.is_empty() || name.contains(['\r', '\n']) {
                return fail("Private directory owner identity invalid");
            }
            let argv = vec![
                path.as_os_str().into(),
                "/inheritance:r".into(),
                "/grant:r".into(),
                format!("{name}:(OI)(CI)F").into(),
            ];
            execute_with(request("icacls", argv, &environment), 0, None, &mut execute)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(owned)
    }
    fn finish(self) -> Result<()> {
        fs::remove_dir_all(&self.0)?;
        Ok(())
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn request(program: &str, args: Vec<OsString>, env: &BTreeMap<String, String>) -> ToolRequest {
    ToolRequest {
        program: program.into(),
        args,
        environment: env.clone(),
        input: Vec::new(),
        seconds: 60,
    }
}
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
#[path = "native_release_signing.rs"]
mod native_release_signing;
pub use native_release_signing::{Certificate, certificate, native_macos_with, native_sign};
#[path = "signed_build_manifest.rs"]
mod signed_build_manifest;
pub use signed_build_manifest::{
    installed_manifest, installed_manifest_with_mode, validate_archive_build,
};
pub fn promotion(documents: &Value, sha: &str, run: u64, branch: &str) -> Result<String> {
    let base = documents["branch"]["commit"]["sha"]
        .as_str()
        .ok_or("Main source SHA missing")?;
    let empty = |v: &Value| {
        v.as_array().is_some_and(|pages| {
            !pages.is_empty()
                && pages
                    .iter()
                    .all(|p| p.as_array().is_some_and(Vec::is_empty))
        })
    };
    let approved = ["main", "migration/native-runtime-checkpoint-20261012"].contains(&branch);
    let record = &documents["run"];
    let required = documents["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|p| p["jobs"].as_array().into_iter().flatten())
        .filter(|j| j["name"] == "Required verification")
        .collect::<Vec<_>>();
    let comparison = &documents["comparison"];
    if !approved
        || !super::canonical_sha(sha)
        || !super::canonical_sha(base)
        || documents["branch"]["protected"] != false
        || !empty(&documents["rules"])
        || !empty(&documents["rulesets"])
        || record["id"] != run
        || record["head_sha"] != sha
        || record["event"] != "workflow_dispatch"
        || record["actor"]["login"] != "KiaroSama"
        || record["head_repository"]["full_name"] != "KiaroSama/Numera-Browser"
        || record["head_branch"] != branch
        || required.len() != 1
        || required[0]["conclusion"] != "success"
        || comparison["base_commit"]["sha"] != base
        || !["ahead", "identical"].contains(&comparison["status"].as_str().unwrap_or(""))
        || comparison["commits"]
            .as_array()
            .and_then(|a| a.last())
            .map(|c| &c["sha"])
            != Some(&Value::String(sha.into()))
    {
        return fail("Promotion refuses unverified or unsafe source state");
    }
    Ok(base.into())
}

/// Read-only operations never load a private signing environment value.
pub fn run(root: &Path, arguments: &[OsString]) -> Result<()> {
    let text = arguments
        .iter()
        .map(|s| s.to_str().ok_or("Release argument must be Unicode"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let operation = *text.first().ok_or("Release operation required")?;
    match (operation, &text[1..]) {
        ("manifest", [directory, destination, generation, source]) => {
            super::version::verify(root)?;
            release_manifest(
                Path::new(directory),
                Path::new(destination),
                generation.parse()?,
                source,
                env!("CARGO_PKG_VERSION"),
                &public_key(&root.join("docs/release-signing-key.pem"))?,
            )?;
            Ok(())
        }
        ("promotion-gate", [directory, source, run, branch]) => {
            let mut documents = serde_json::Map::new();
            for name in ["branch", "rules", "rulesets", "run", "jobs", "comparison"] {
                documents.insert(
                    name.into(),
                    serde_json::from_slice(&fs::read(
                        Path::new(directory).join(format!("{name}.json")),
                    )?)?,
                );
            }
            println!(
                "{}",
                json!({"base_sha":promotion(&Value::Object(documents),source,run.parse()?,branch)?})
            );
            Ok(())
        }
        ("plan", []) => {
            println!("{}", super::release::build_graph(root)?);
            Ok(())
        }
        ("verify", [manifest, signature, public]) => verify_manifest(
            Path::new(manifest),
            Path::new(signature),
            &public_key(Path::new(public))?,
        ),
        ("sign", [manifest, signature, public]) => {
            let environment = std::env::vars().collect::<BTreeMap<_, _>>();
            owner_context(&environment)?;
            let mut raw = environment
                .get("NUMERA_RELEASE_SIGNING_KEY")
                .ok_or("Protected release signing input missing")?
                .as_bytes()
                .to_vec();
            let key = private_key(&raw);
            raw.fill(0);
            sign_manifest(
                Path::new(manifest),
                Path::new(signature),
                &key?,
                &public_key(Path::new(public))?,
            )
        }
        ("distribution-verify", [directory, manifest, source]) => {
            super::release::verify_distribution(
                Path::new(directory),
                &serde_json::from_slice(&fs::read(manifest)?)?,
                source,
                true,
            )
        }
        ("archive", [stage, destination]) => {
            super::release::archive_stage(Path::new(stage), Path::new(destination))
        }
        ("assemble", [stage]) => super::release::assemble_stage(Path::new(stage)),
        ("identity-prepare", [directory]) => {
            let env = std::env::vars().collect();
            let source = String::from_utf8(super::git(root, &["rev-parse", "HEAD"])?)?;
            let record = super::release::build_identity(root, source.trim(), &env)?;
            super::release::prepare_identity(Path::new(directory), record, None)?;
            Ok(())
        }
        ("identity-verify", [stage]) => {
            let env = std::env::vars().collect();
            let source = String::from_utf8(super::git(root, &["rev-parse", "HEAD"])?)?;
            let expected = super::release::build_identity(root, source.trim(), &env)?;
            super::release::verify_embedded_with(Path::new(stage), &expected, &env, |path, env| {
                let mut argv = vec!["--build-info".into()];
                let mut req = request(
                    path.to_str()
                        .ok_or("Identity executable path not Unicode")?,
                    std::mem::take(&mut argv),
                    env,
                );
                req.seconds = 15;
                execute_with(req, 0, None, &mut execute)
            })
        }
        ("inventory", [stage, platform, source, generation, version]) => {
            installed_manifest(
                Path::new(stage),
                platform,
                source,
                generation.parse()?,
                version,
                json!({"sdk":{},"browsers":{}}),
            )?;
            Ok(())
        }
        ("native-sign", [stage, platform, public]) => {
            let env = std::env::vars().collect::<BTreeMap<_, _>>();
            let private = std::env::var_os("RUNNER_TEMP")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join(".ci-work"));
            native_sign(
                Path::new(stage),
                platform,
                Path::new(public),
                &private,
                &env,
            )
        }
        _ => fail("Unknown release operation or argument count"),
    }
}

pub fn smoke_with<F>(
    program: &Path,
    arguments: &[OsString],
    input: &[u8],
    environment: &BTreeMap<String, String>,
    seconds: u64,
    mut tool: F,
) -> Result<Value>
where
    F: FnMut(&ToolRequest) -> io::Result<ToolOutput>,
{
    let mut call = request(
        program
            .to_str()
            .ok_or("Smoke executable path not Unicode")?,
        arguments.to_vec(),
        environment,
    );
    call.input = input.to_vec();
    call.seconds = seconds;
    let bytes = execute_with(call, 0, None, &mut tool)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn release_manifest(
    directory: &Path,
    destination: &Path,
    generation: u64,
    source: &str,
    version: &str,
    key: &VerifyingKey,
) -> Result<Value> {
    if generation == 0 || !super::canonical_sha(source) {
        return fail("Signed release generation and source identity invalid");
    }
    let distribution: Value =
        serde_json::from_slice(&fs::read(directory.join("distribution.json"))?)?;
    super::release::verify_distribution(directory, &distribution, source, true)?;
    let mut assets = Vec::new();
    let mut components = BTreeMap::new();
    for platform in super::release::RELEASE_PLATFORMS {
        let metadata: Value =
            serde_json::from_slice(&fs::read(directory.join(format!("{platform}.json")))?)?;
        if metadata["source_sha"] != source
            || metadata["platform"] != platform
            || metadata["signing_scope"] != "owner"
        {
            return fail("Update component source/platform or owner signing scope differs");
        }
        let asset = &metadata["asset"];
        if !distribution["assets"]
            .as_array()
            .is_some_and(|values| values.contains(asset))
        {
            return fail("Update archive receipt differs from exact verified distribution");
        }
        let name = asset["name"].as_str().ok_or("Asset name missing")?;
        validate_archive_build(
            &directory.join(name),
            &metadata,
            platform,
            generation,
            source,
            key,
        )?;
        let mut row = asset.clone();
        row["platform"] = platform.into();
        row["archive_root"] = "Numera Browser".into();
        for field in ["expanded_size", "max_members", "build_manifest_sha256"] {
            row[field] = metadata[field].clone();
        }
        assets.push(row);
        components.insert(platform, metadata["components"].clone());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let value = json!({"schema":1,"repository":"KiaroSama/Numera-Browser","product":"Numera Browser","version":version,"source_sha":source,"generation":generation,"created_at":now,"expires_at":now+90*24*60*60,"minimum_protocol":1,"assets":assets,"components":components});
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() > 1024 * 1024 {
        return fail("Release manifest exceeds signed bound");
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(value)
}
