//! Explicit manual release tools. No ambient owner key is read by validation.
use super::{Result, fail, regular, sha};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{self, Read, Write},
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
fn private_key(text: &[u8]) -> Result<SigningKey> {
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
    use numera_core::ownership::{capture::Pipe, tree::OwnedTree};
    use std::{
        process::{Command, Stdio},
        time::Instant,
    };
    retry_tool_cleanup()?;
    if request.input.len() > 4096 || request.seconds == 0 {
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
                            if outputs[0].len() + outputs[1].len() + count > 2 * 1024 * 1024 {
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
fn macho_tamper(bytes: &[u8]) -> Result<usize> {
    let word = |offset: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or("MachO command truncated")?
                .try_into()?,
        ))
    };
    if bytes.len() < 32 || word(0)? != 0xfeedfacf || word(4)? != 0x0100000c {
        return fail("Native executable has no bounded signed content for tamper check");
    }
    let count = word(16)? as usize;
    let size = word(20)? as usize;
    if size > 1024 * 1024 || count > size / 8 || 32 + size > bytes.len() {
        return fail("MachO signature command bounds differ");
    }
    let mut cursor = 32;
    let mut offset = None;
    for _ in 0..count {
        let kind = word(cursor)?;
        let length = word(cursor + 4)? as usize;
        if length < 8 || cursor + length > 32 + size {
            return fail("MachO command bounds differ");
        }
        if kind == 0x1d {
            if offset.is_some() || length != 16 {
                return fail("MachO signature command duplicated");
            }
            let start = word(cursor + 8)? as usize;
            let len = word(cursor + 12)? as usize;
            if start <= 32 + size || len == 0 || start + len > bytes.len() {
                return fail("MachO signed content bounds differ");
            }
            offset = Some(start - 1);
        }
        cursor += length;
    }
    if cursor != 32 + size {
        return fail("MachO load command length differs");
    }
    offset.ok_or_else(|| "Native executable has no bounded signed content for tamper check".into())
}
#[derive(Clone, Debug)]
pub struct Certificate {
    pub sha256: String,
    pub sha1: String,
}
pub fn native_macos_with<F>(
    stage: &Path,
    private_root: &Path,
    certificate: &Certificate,
    transport: &[u8],
    env: &BTreeMap<String, String>,
    mut tool: F,
) -> Result<()>
where
    F: FnMut(&ToolRequest) -> io::Result<ToolOutput>,
{
    let private = Private::new(private_root)?;
    let transport_path = private.0.join("transport.p12");
    fs::write(&transport_path, transport)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&transport_path, fs::Permissions::from_mode(0o600))?;
    }
    let keychain = private.0.join("signing.keychain-db");
    let mut create = args(&["create-keychain", "-p", ""]);
    create.push(keychain.clone().into());
    execute_with(request("security", create, env), 0, None, &mut tool)?;
    let result = (|| -> Result<()> {
        let mut unlock = args(&["unlock-keychain", "-p", ""]);
        unlock.push(keychain.clone().into());
        execute_with(request("security", unlock, env), 0, None, &mut tool)?;
        let mut import = args(&["import"]);
        import.extend([
            transport_path.clone().into(),
            "-k".into(),
            keychain.clone().into(),
            "-P".into(),
            "".into(),
            "-T".into(),
            "/usr/bin/codesign".into(),
        ]);
        execute_with(request("security", import, env), 0, None, &mut tool)?;
        let mut partition = args(&[
            "set-key-partition-list",
            "-S",
            "apple-tool:,apple:,codesign:",
            "-s",
            "-k",
            "",
        ]);
        partition.push(keychain.clone().into());
        execute_with(request("security", partition, env), 0, None, &mut tool)?;
        let approved = format!("=certificate leaf = H\"{}\"", certificate.sha1);
        let wrong = format!(
            "=certificate leaf = H\"{}{}\"",
            if certificate.sha1.starts_with('0') {
                '1'
            } else {
                '0'
            },
            &certificate.sha1[1..]
        );
        let verify = |path: &Path, requirement: &str| {
            let mut argv = args(&["--verify", "--strict", "--test-requirement", requirement]);
            argv.push(path.as_os_str().into());
            request("codesign", argv, env)
        };
        let paths = [stage.join("numera"), stage.join("engine/numera-engine")];
        for path in &paths {
            regular(path, false)?;
            let mut sign = args(&["--force", "--sign", &certificate.sha1, "--keychain"]);
            sign.extend([
                keychain.clone().into(),
                "--timestamp=none".into(),
                path.as_os_str().into(),
            ]);
            execute_with(request("codesign", sign, env), 0, None, &mut tool)?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
            execute_with(verify(path, &wrong), 3, None, &mut tool)?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
        }
        for (index, path) in paths.iter().enumerate() {
            let mut bytes = fs::read(path)?;
            let offset = macho_tamper(&bytes)?;
            bytes[offset] ^= 1;
            let tampered = private.0.join(format!("tampered-{index}"));
            fs::write(&tampered, bytes)?;
            execute_with(
                verify(&tampered, &approved),
                1,
                Some(b"code or signature modified"),
                &mut tool,
            )?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
        }
        Ok(())
    })();
    let mut delete = args(&["delete-keychain"]);
    delete.push(keychain.into());
    let cleanup = execute_with(request("security", delete, env), 0, None, &mut tool);
    result?;
    cleanup?;
    private.finish()?;
    fs::write(
        stage.join("SIGNING.json"),
        serde_json::to_vec(
            &json!({"owner":"KiaroSama","certificate_sha256":certificate.sha256,"platform":"macos-arm64","trust":"owner-self-signed-private-trust","notarized":false,"publicly_trusted":false}),
        )?,
    )?;
    Ok(())
}
pub fn installed_manifest(
    stage: &Path,
    platform: &str,
    source: &str,
    generation: u64,
    version: &str,
    components: Value,
) -> Result<Value> {
    installed_manifest_with_mode(
        stage,
        platform,
        source,
        generation,
        version,
        components,
        |path| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                Ok(fs::metadata(path)?.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                Ok(path.extension().is_some_and(|ext| ext == "exe"))
            }
        },
    )
}
pub fn installed_manifest_with_mode(
    stage: &Path,
    platform: &str,
    source: &str,
    generation: u64,
    version: &str,
    components: Value,
    executable: impl Fn(&Path) -> io::Result<bool>,
) -> Result<Value> {
    if generation == 0 || !super::canonical_sha(source) {
        return fail("Installed build generation requires actual hosted run identity");
    }
    let mut rows = Vec::new();
    for (name, path) in super::release::files(stage)? {
        if ["BUILD.json", "BUILD.sig"].contains(&name.as_str()) {
            continue;
        }
        super::release::public_path(&name, true)?;
        let executable = executable(&path)?;
        rows.push(json!({"path":name,"size":fs::metadata(&path)?.len(),"sha256":super::digest(&path)?,"executable":executable}));
    }
    let value = json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser","version":version,"source_sha":source,"generation":generation,"platform":platform,"minimum_protocol":1,"components":components,"files":rows});
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return fail("Installed immutable inventory exceeds signed manifest bound");
    }
    fs::write(stage.join("BUILD.json"), bytes)?;
    Ok(value)
}
pub fn validate_archive_build(
    archive: &Path,
    metadata: &Value,
    platform: &str,
    generation: u64,
    source: &str,
    key: &VerifyingKey,
) -> Result<()> {
    let mut contents = BTreeMap::new();
    let mut count = 0u64;
    let mut expanded = 0u64;
    let mut accept = |name: &str, size: u64, mut input: Box<dyn Read + '_>| -> Result<()> {
        count += 1;
        expanded = expanded
            .checked_add(size)
            .ok_or("Archive expanded size overflow")?;
        if ["Numera Browser/BUILD.json", "Numera Browser/BUILD.sig"].contains(&name) {
            let limit = if name.ends_with(".sig") {
                64
            } else {
                8 * 1024 * 1024
            };
            if size > limit || contents.contains_key(name) {
                return fail("Packaged signed inventory duplicated or oversized");
            }
            let mut bytes = Vec::new();
            input.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != size {
                return fail("Packaged inventory length differs");
            }
            contents.insert(name.to_owned(), bytes);
        }
        Ok(())
    };
    if archive.extension().is_some_and(|s| s == "zip") {
        let mut packed = zip::ZipArchive::new(fs::File::open(archive)?)?;
        for i in 0..packed.len() {
            let file = packed.by_index(i)?;
            let name = file.name()?.into_owned();
            let size = file.size();
            accept(&name, size, Box::new(file))?;
        }
    } else {
        let mut packed = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?));
        for file in packed.entries()? {
            let file = file?;
            let name = file.path()?.to_string_lossy().into_owned();
            let size = if file.header().entry_type().is_file() {
                file.size()
            } else {
                0
            };
            if name.ends_with("BUILD.json") && !file.header().entry_type().is_file() {
                return fail("Packaged inventory linked");
            }
            accept(&name, size, Box::new(file))?;
        }
    }
    if count
        > metadata["max_members"]
            .as_u64()
            .ok_or("Archive member limit missing")?
        || expanded
            > metadata["expanded_size"]
                .as_u64()
                .ok_or("Archive expansion limit missing")?
    {
        return fail("Packaged archive exceeds signed expansion bounds");
    }
    let bytes = contents
        .get("Numera Browser/BUILD.json")
        .ok_or("Packaged signed inventory missing")?;
    let proof = contents
        .get("Numera Browser/BUILD.sig")
        .ok_or("Packaged signed inventory missing")?;
    if proof.len() != 64 || sha(bytes) != metadata["build_manifest_sha256"] {
        return fail("Packaged inventory hash/signature differs");
    }
    key.verify_strict(bytes, &Signature::from_slice(proof)?)
        .map_err(|_| "Packaged build inventory is not authenticated by owner")?;
    let value: Value = serde_json::from_slice(bytes)?;
    if value["generation"] != generation
        || value["source_sha"] != source
        || value["platform"] != platform
        || value["components"] != metadata["components"]
    {
        return fail("Packaged build generation, source or SDK compatibility differs");
    }
    Ok(())
}
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

pub fn certificate(path: &Path, environment: &BTreeMap<String, String>) -> Result<Certificate> {
    regular(path, false)?;
    let invoke = |extra: &[&str]| -> Result<String> {
        let mut argv = args(&["x509", "-in"]);
        argv.push(path.as_os_str().into());
        argv.extend(args(extra));
        let bytes = execute_with(request("openssl", argv, environment), 0, None, &mut execute)?;
        Ok(String::from_utf8(bytes)?)
    };
    let subject = invoke(&["-noout", "-subject", "-nameopt", "RFC2253"])?;
    if subject.trim() != "subject=CN=KiaroSama" {
        return fail("Code signing certificate owner differs from approved identity");
    }
    let usage = invoke(&["-noout", "-ext", "extendedKeyUsage"])?;
    if !usage.contains("Code Signing") {
        return fail("Code signing certificate lacks required usage");
    }
    let mut verify = args(&["verify", "-CAfile"]);
    verify.extend([path.as_os_str().into(), path.as_os_str().into()]);
    execute_with(
        request("openssl", verify, environment),
        0,
        None,
        &mut execute,
    )?;
    let fingerprint = |algorithm: &str| -> Result<String> {
        let value = invoke(&["-noout", "-fingerprint", algorithm])?;
        let hex = value
            .trim()
            .split_once('=')
            .ok_or("Certificate fingerprint missing")?
            .1
            .replace(':', "")
            .to_lowercase();
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return fail("Certificate fingerprint malformed");
        }
        Ok(hex)
    };
    let info = Certificate {
        sha256: fingerprint("-sha256")?,
        sha1: fingerprint("-sha1")?,
    };
    if info.sha256.len() != 64 || info.sha1.len() != 40 {
        return fail("Certificate fingerprint size differs");
    }
    Ok(info)
}
pub fn native_sign(
    stage: &Path,
    platform: &str,
    public_certificate: &Path,
    private_root: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    if platform == "linux-x64" {
        return Ok(());
    }
    owner_context(environment)?;
    if !["macos-arm64", "windows-x64"].contains(&platform) {
        return fail("Native signing platform unsupported");
    }
    let expected = certificate(public_certificate, environment)?;
    let payload = environment
        .get("NUMERA_CODE_SIGNING_PFX")
        .filter(|s| !s.is_empty())
        .ok_or("Protected native signing input missing")?;
    let password = environment
        .get("NUMERA_CODE_SIGNING_PASSWORD")
        .filter(|s| !s.is_empty())
        .ok_or("Protected native signing input missing")?;
    if password.len() > 4095 || password.contains(['\n', '\r']) {
        return fail("Protected native signing password cannot be transported");
    }
    let version = (platform == "windows-x64").then_some(env!("CARGO_PKG_VERSION"));
    let mut raw = STANDARD
        .decode(payload)
        .map_err(|_| "Protected native signing input could not be decoded")?;
    if let Some(version) = version {
        let result = numera_core::native_windows_sign::sign(
            stage,
            &raw,
            password,
            &expected.sha256,
            version,
        );
        raw.fill(0);
        let receipt = result?;
        fs::write(stage.join("SIGNING.json"), serde_json::to_vec(&receipt)?)?;
        return Ok(());
    }
    let private = Private::new(private_root)?;
    let identity = private.0.join("identity.pfx");
    let pem_path = private.0.join("identity.pem");
    let transport = private.0.join("transport.p12");
    let written = fs::write(&identity, &raw);
    raw.fill(0);
    written?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&identity, fs::Permissions::from_mode(0o600))?;
    }
    let mut open = args(&["pkcs12", "-in"]);
    open.extend([
        identity.clone().into(),
        "-out".into(),
        pem_path.clone().into(),
        "-nodes".into(),
        "-passin".into(),
        "stdin".into(),
    ]);
    let mut opening = request("openssl", open, environment);
    opening.input = password.as_bytes().to_vec();
    opening.input.push(b'\n');
    execute_with(opening, 0, None, &mut execute)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&pem_path, fs::Permissions::from_mode(0o600))?;
    }
    let actual = certificate(&pem_path, environment)?;
    if actual.sha256 != expected.sha256 {
        return fail("Protected certificate differs from pinned approved code signer");
    }
    let mut export = args(&["pkcs12", "-export", "-in"]);
    export.extend([
        pem_path.into(),
        "-out".into(),
        transport.clone().into(),
        "-passout".into(),
        "pass:".into(),
    ]);
    execute_with(
        request("openssl", export, environment),
        0,
        None,
        &mut execute,
    )?;
    let mut bytes = fs::read(transport)?;
    private.finish()?;
    let result = native_macos_with(stage, private_root, &expected, &bytes, environment, execute);
    bytes.fill(0);
    result
}
/// Read-only/default operations never load a private signing environment value.
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
    for platform in ["windows-x64", "linux-x64", "macos-arm64"] {
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
