//! Release checks are read-only contracts. This module never publishes or signs
//! with an ambient owner credential.
use super::{Result, canonical_sha, digest, fail, regular};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};
pub fn public_path(name: &str, vendor: bool) -> Result<()> {
    let parts = name.split('/').collect::<Vec<_>>();
    let forbidden = [
        ".ai",
        ".specify",
        "specs",
        ".git",
        ".claude",
        ".agents",
        ".kiro",
        ".codex",
        ".cursor",
        ".cline",
        ".codebase-memory",
        "graphify-out",
        ".ignoreme",
        "__pycache__",
        ".venv",
        "venv",
    ];
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || forbidden.contains(part)
                || (!vendor && *part == "node_modules")
                || (part.starts_with(".env")
                    && ![".env.example", ".env.sample", ".env.template", ".env.dist"]
                        .contains(part))
        })
        || ["data", "plans", ".ci-work", ".ci-cache", ".ci-runner-win"].contains(&parts[0])
        || ["CLAUDE.md", "AGENTS.md"].contains(&name)
        || ["secrets.md", "explain-ai.md", "reference.md"].contains(
            &parts
                .last()
                .copied()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        )
    {
        return fail("Refusing non-public publication path");
    }
    Ok(())
}
pub fn asset(path: &Path) -> Result<Value> {
    regular(path, false)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Asset filename is not Unicode")?;
    if !regex::Regex::new(r"^numera-[a-z0-9][a-z0-9._-]*\.(?:tar\.gz|zip|whl)$")?.is_match(name) {
        return fail("Invalid release asset name or type");
    }
    Ok(serde_json::json!({"name":name,"size":fs::metadata(path)?.len(),"sha256":digest(path)?}))
}
pub fn verify_distribution(
    directory: &Path,
    manifest: &Value,
    sha: &str,
    allow_metadata: bool,
) -> Result<()> {
    if !canonical_sha(sha) || manifest["schema"] != 1 || manifest["source_sha"] != sha {
        return fail("Distribution manifest source SHA does not match");
    }
    let mut expected = BTreeSet::new();
    let asset_name = regex::Regex::new(r"^numera-[a-z0-9][a-z0-9._-]*\.(?:tar\.gz|zip|whl)$")?;
    for row in manifest["assets"]
        .as_array()
        .ok_or("Distribution assets missing")?
    {
        let name = row["name"].as_str().ok_or("Asset name missing")?;
        if !asset_name.is_match(name) {
            return fail("Invalid release asset name");
        }
        if !expected.insert(name.to_owned()) {
            return fail("Distribution duplicate asset name");
        }
        if asset(&directory.join(name))? != *row {
            return fail("Distribution checksum or size differs");
        }
    }
    if expected.is_empty() {
        return fail("Distribution manifest has no assets");
    }
    for entry in fs::read_dir(directory)? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| "Distribution name is not Unicode")?;
        let metadata = allow_metadata
            && [
                "windows-x64.json",
                "linux-x64.json",
                "macos-arm64.json",
                "server.json",
            ]
            .contains(&name.as_str());
        if !expected.contains(&name)
            && !["distribution.json", "SHA256SUMS"].contains(&name.as_str())
            && !metadata
        {
            return fail("Distribution contains unexpected files");
        }
    }
    Ok(())
}
pub fn architecture(bytes: &[u8], platform: &str) -> Result<()> {
    let valid = match platform {
        "linux-x64" => {
            bytes.len() >= 20
                && bytes[..6] == *b"\x7fELF\x02\x01"
                && u16::from_le_bytes([bytes[18], bytes[19]]) == 62
        }
        "macos-arm64" => {
            bytes.len() >= 8
                && bytes[..4] == [0xcf, 0xfa, 0xed, 0xfe]
                && u32::from_le_bytes(bytes[4..8].try_into()?) == 0x0100000c
        }
        "windows-x64" => {
            if bytes.len() < 64 || &bytes[..2] != b"MZ" {
                false
            } else {
                let offset = u32::from_le_bytes(bytes[60..64].try_into()?) as usize;
                bytes.get(offset..offset + 6).is_some_and(|pe| {
                    pe[..4] == *b"PE\0\0" && u16::from_le_bytes([pe[4], pe[5]]) == 0x8664
                })
            }
        }
        _ => false,
    };
    if !valid {
        return fail("Portable shell architecture differs from its download label");
    }
    Ok(())
}
pub fn generation(value: &str, hosted: bool) -> Result<u64> {
    if value.is_empty()
        || value.len() > 16
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return fail("Build generation is not canonical");
    }
    let number = value.parse()?;
    if hosted && number == 0 {
        return fail("Hosted build requires actual positive generation");
    }
    Ok(number)
}
pub fn identity(
    info: &Value,
    sha: &str,
    generation: u64,
    version: &str,
    acceptance: bool,
) -> Result<()> {
    if info["schema"] != 1
        || info["owner"] != "KiaroSama"
        || info["product"] != "Numera Browser"
        || info["version"] != version
        || info["source_sha"] != sha
        || info["generation"] != generation
        || !canonical_sha(sha)
        || info
            .get("acceptance")
            .is_some_and(|value| value != acceptance)
    {
        return fail("Embedded executable identity differs from exact build source");
    }
    Ok(())
}

pub fn validate_stage(stage: &Path, sha: &str) -> Result<()> {
    regular(&stage.join("LICENSE"), false)
        .map_err(|_| "Product archive is missing its full LICENSE")?;
    regular(
        &stage
            .join("sources")
            .join(format!("numera-source-{sha}.tar.gz")),
        false,
    )
    .map_err(|_| "Product archive is missing matching complete source")?;
    let browsers = stage.join("browsers");
    if browsers.is_dir() {
        let manifest: Value = serde_json::from_slice(&fs::read(browsers.join("manifest.json"))?)?;
        if manifest.get("cloak").is_some() || browsers.join("cloak").exists() {
            return fail("Cloak binary redistribution is not authorized");
        }
    }
    Ok(())
}
pub fn update_state(name: &str) -> bool {
    name == ".updates" || name == ".numera-update.lock" || name.starts_with(".numera-update-")
}
pub fn files(root: &Path) -> Result<Vec<(String, std::path::PathBuf)>> {
    fn walk(
        base: &Path,
        path: &Path,
        rows: &mut Vec<(String, std::path::PathBuf)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 {
            return fail("Release resource nesting exceeds bound");
        }
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = path
                .strip_prefix(base)?
                .to_str()
                .ok_or("Release resource is not Unicode")?
                .replace('\\', "/");
            if update_state(name.split('/').next().unwrap_or("")) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return fail("Release resources refuse linked files");
            }
            if metadata.is_dir() {
                walk(base, &path, rows, depth + 1)?;
            } else if metadata.is_file() {
                rows.push((name, path));
            } else {
                return fail("Release resource is not a regular file");
            }
        }
        Ok(())
    }
    regular(root, true)?;
    let mut rows = Vec::new();
    walk(root, root, &mut rows, 0)?;
    Ok(rows)
}
pub fn archive_stage(stage: &Path, destination: &Path) -> Result<()> {
    archive_stage_with_mode(stage, destination, |_, _| None)
}
pub fn archive_stage_with_mode(
    stage: &Path,
    destination: &Path,
    mode: impl Fn(&Path, &fs::Metadata) -> Option<u32>,
) -> Result<()> {
    let rows = files(stage)?;
    let root = stage
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Archive root missing")?;
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| -> Result<()> {
        if destination.to_string_lossy().ends_with(".tar.gz") {
            let compressor = flate2::write::GzEncoder::new(output, flate2::Compression::default());
            let mut archive = tar::Builder::new(compressor);
            for (name, path) in rows {
                let metadata = fs::metadata(&path)?;
                let mut header = tar::Header::new_gnu();
                header.set_metadata(&metadata);
                if let Some(mode) = mode(&path, &metadata) {
                    header.set_mode(mode);
                }
                header.set_path(format!("{root}/{name}"))?;
                // PAX retains subsecond timestamps; the GNU header remains valid for older readers.
                let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
                let timestamp = format!("{}.{:09}", modified.as_secs(), modified.subsec_nanos());
                archive.append_pax_extensions([("mtime", timestamp.as_bytes())])?;
                header.set_cksum();
                archive.append(&header, fs::File::open(path)?)?;
            }
            archive.into_inner()?.finish()?.sync_all()?;
        } else if destination.extension().is_some_and(|e| e == "zip") {
            let mut archive = zip::ZipWriter::new(output);
            for (name, path) in rows {
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    fs::metadata(&path)?.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0o644;
                archive.start_file(
                    format!("{root}/{name}"),
                    zip::write::SimpleFileOptions::default().unix_permissions(mode),
                )?;
                std::io::copy(&mut fs::File::open(path)?, &mut archive)?;
            }
            archive.finish()?.sync_all()?;
        } else {
            return fail("Unsupported distribution archive type");
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}
/// Preserve portable user data before removing old product bytes. A failed wipe
/// restores that same directory before returning; no build may continue stale.
pub fn assemble_stage_with(
    stage: &Path,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<()> {
    let parent = stage.parent().ok_or("Stage parent missing")?;
    fs::create_dir_all(parent)?;
    regular(parent, true)?;
    if stage.exists() {
        regular(stage, true)?;
    }
    let saved = parent.join(format!(
        ".numera-data-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let data = stage.join("data");
    let retained = data.exists();
    if retained {
        regular(&data, true)?;
        fs::rename(&data, &saved)?;
    }
    let cleared = if stage.exists() {
        remove(stage)
    } else {
        Ok(())
    };
    if let Err(error) = cleared {
        fs::create_dir_all(stage)?;
        if retained {
            fs::rename(&saved,&data).map_err(|restore|format!("close Numera before rebuilding; data retained at {}: {restore}; original: {error}",saved.display()))?;
        }
        return fail(format!(
            "close Numera before rebuilding; old stage could not be cleared: {error}"
        ));
    }
    fs::create_dir_all(stage)?;
    if retained {
        fs::rename(&saved, &data)?;
    }
    Ok(())
}
pub fn assemble_stage(stage: &Path) -> Result<()> {
    assemble_stage_with(stage, |path| fs::remove_dir_all(path))
}
pub fn build_identity(
    root: &Path,
    sha: &str,
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    if !canonical_sha(sha)
        || environment
            .get("GITHUB_SHA")
            .is_some_and(|expected| expected != sha)
    {
        return fail("Build identity checkout differs from its source SHA");
    }
    let dashboard: Value = serde_json::from_slice(&fs::read(root.join("dashboard/package.json"))?)?;
    let version = dashboard["version"]
        .as_str()
        .ok_or("Build version missing")?;
    let expression = regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$")?;
    if !expression.is_match(version) || version.split('.').any(|n| n.parse::<u16>().is_err()) {
        return fail("Unsupported three-part product version");
    }
    let version_declaration = regex::Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#)?;
    for name in ["engine-rust/Cargo.toml", "desktop/src-tauri/Cargo.toml"] {
        let text = fs::read_to_string(root.join(name))?;
        let declared = version_declaration
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str());
        if declared != Some(version) {
            return fail("Build identity product versions disagree");
        }
    }
    let config: Value =
        serde_json::from_slice(&fs::read(root.join("desktop/src-tauri/tauri.conf.json"))?)?;
    if config["version"] != version
        || config["productName"] != "Numera Browser"
        || config["bundle"]["publisher"] != "KiaroSama"
    {
        return fail("Desktop identity differs from approved product and owner");
    }
    let generation = generation(
        environment.get("GITHUB_RUN_ID").map_or("0", String::as_str),
        environment
            .get("GITHUB_ACTIONS")
            .is_some_and(|v| v == "true"),
    )?;
    Ok(
        serde_json::json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser","version":version,"source_sha":sha,"generation":generation}),
    )
}
pub fn prepare_identity(
    directory: &Path,
    mut record: Value,
    acceptance: Option<u64>,
) -> Result<Value> {
    if let Some(next) = acceptance {
        let original = record["generation"]
            .as_u64()
            .ok_or("Build generation missing")?;
        if original == 0
            || ![
                original.checked_mul(2),
                original.checked_mul(2).and_then(|v| v.checked_add(1)),
            ]
            .contains(&Some(next))
        {
            return fail("Acceptance requires actual hosted fixture generation");
        }
        record["generation"] = next.into();
        record["acceptance"] = true.into();
    }
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("build-identity.json"),
        serde_json::to_vec(&record)?,
    )?;
    let version = record["version"].as_str().ok_or("Version missing")?;
    let resource = serde_json::json!({"CompanyName":"KiaroSama","FileDescription":"Numera Browser engine","filevers":version.split('.').map(str::parse::<u16>).collect::<std::result::Result<Vec<_>,_>>()?.into_iter().chain([0]).collect::<Vec<_>>(),"PrivateBuild":record["source_sha"],"ProductVersion":version,"OriginalFilename":"numera-engine.exe"});
    fs::write(
        directory.join("windows-version.json"),
        serde_json::to_vec(&resource)?,
    )?;
    Ok(record)
}
pub fn verify_embedded_with<F>(
    stage: &Path,
    expected: &Value,
    environment: &std::collections::BTreeMap<String, String>,
    mut tool: F,
) -> Result<()>
where
    F: FnMut(&Path, &std::collections::BTreeMap<String, String>) -> Result<Vec<u8>>,
{
    let clean = super::release_signing::tool_environment(environment, "identity");
    for relative in if cfg!(windows) {
        ["numera.exe", "engine/numera-engine.exe"]
    } else {
        ["numera", "engine/numera-engine"]
    } {
        let path = stage.join(relative);
        regular(&path, false)?;
        let bytes = tool(&path, &clean)?;
        if bytes.len() > 4096 {
            return fail("Embedded executable identity differs from exact build source");
        }
        let mut actual: Value = serde_json::from_slice(&bytes)?;
        if actual.get("application_runtime").is_some() {
            if actual["application_runtime"] != "native" || actual["acceptance"] != false {
                return fail("Embedded executable identity differs from exact build source");
            }
            actual
                .as_object_mut()
                .ok_or("Embedded identity is not an object")?
                .remove("application_runtime");
            actual
                .as_object_mut()
                .ok_or("Embedded identity is not an object")?
                .remove("acceptance");
        }
        if actual != *expected {
            return fail("Embedded executable identity differs from exact build source");
        }
    }
    Ok(())
}

/// A portable build resolves its explicitly bundled browser through the same
/// resource selector as the shipping engine, respecting caller overrides.
pub fn bundled_browser(
    app: &Path,
    data: &Path,
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<numera_core::browser::bundled::Resources> {
    let root = numera_core::browser::bundled::discover(app)?;
    Ok(numera_core::browser::bundled::select(
        root.as_deref(),
        "playwright",
        data,
        environment,
        &[],
    )?)
}
pub fn build_graph(root: &Path) -> Result<Value> {
    let manifest = fs::read_to_string(root.join("engine-rust/Cargo.toml"))?;
    let binary=regex::Regex::new(r#"(?s)\[\[bin\]\]\s*name\s*=\s*"numera-engine"\s*path\s*=\s*"([^"]+)"\s*required-features\s*=\s*\["native-product"\]"#)?.captures(&manifest).and_then(|c|c.get(1)).map(|v|v.as_str()).ok_or("Native engine build entry missing")?;
    let entry = root.join("engine-rust").join(binary);
    regular(&entry, false)?;
    let source = fs::read_to_string(&entry)?;
    if !source.contains("native_entry::") || !manifest.contains("native-browser") {
        return fail("Native engine is missing its launch/API/vault entry");
    }
    Ok(
        serde_json::json!({"engine_entry":binary,"runtime":"rust","launch":true,"api":true,"vault":true,"icon":"desktop/src-tauri/icons/icon.ico"}),
    )
}
pub fn release_workflows(workflows: &Value) -> Result<()> {
    let ci = &workflows["ci"];
    let release = &workflows["release"];
    if ci["on"]
        .as_object()
        .is_none_or(|on| on.len() != 1 || !on.contains_key("workflow_dispatch"))
        || release["on"].get("workflow_call").is_none()
        || release["on"].get("workflow_run").is_some()
    {
        return fail("Signed artifact review requires approved manual CI and reusable release");
    }
    let attest = &ci["jobs"]["attest"];
    let review = &ci["jobs"]["review"];
    if attest["needs"] != "required"
        || review["needs"] != serde_json::json!(["required", "attest"])
        || review["uses"] != "./.github/workflows/release.yml"
        || ci["permissions"]["contents"] != "read"
        || release["permissions"]["contents"] != "read"
        || ci["jobs"].get("promote").is_some()
        || ci["jobs"].get("publish").is_some()
        || release["jobs"].get("publish").is_some()
    {
        return fail("Signed artifact review requires verified evidence and read-only authority");
    }
    for (guard, terms) in [
        (
            &ci["jobs"]["coverage"]["if"],
            vec![
                "github.actor == 'KiaroSama'",
                "github.repository == 'KiaroSama/Numera-Browser'",
                "inputs.trusted_sha == github.sha",
                "github.ref == 'refs/heads/main'",
                "github.ref == 'refs/heads/migration/native-runtime-checkpoint-20261012'",
            ],
        ),
        (
            &attest["if"],
            vec![
                "github.event_name == 'workflow_dispatch'",
                "github.actor == 'KiaroSama'",
                "inputs.trusted_sha == github.sha",
                "needs.required.result == 'success'",
            ],
        ),
        (
            &review["if"],
            vec![
                "needs.required.result == 'success'",
                "needs.attest.result == 'success'",
            ],
        ),
        (
            &release["jobs"]["review"]["if"],
            vec![
                "github.event_name == 'workflow_dispatch'",
                "github.actor == 'KiaroSama'",
                "github.repository == 'KiaroSama/Numera-Browser'",
                "inputs.source_sha == github.sha",
                "inputs.run_id == github.run_id",
                "github.ref == 'refs/heads/main'",
                "github.ref == 'refs/heads/migration/native-runtime-checkpoint-20261012'",
            ],
        ),
    ] {
        if guard.as_str().is_none_or(|guard| {
            terms.iter().any(|term| !guard.contains(term)) || guard.contains("|| true")
        }) {
            return fail(
                "Manual artifact review owner, repository, source, run or prerequisite guard differs",
            );
        }
    }
    if review["with"]["source_sha"] != "${{ inputs.trusted_sha }}"
        || review["with"]["run_id"] != "${{ github.run_id }}"
    {
        return fail("Artifact review must consume its exact manual source and verification run");
    }
    let prerequisite = [
        "coverage",
        "dashboard",
        "desktop-contracts",
        "docker",
        "codeql",
        "dependencies",
    ];
    for (job, expected) in [
        ("distribution", prerequisite.to_vec()),
        (
            "required",
            prerequisite.into_iter().chain(["distribution"]).collect(),
        ),
    ] {
        let needs = ci["jobs"][job]["needs"]
            .as_array()
            .ok_or("Artifact review prerequisites missing")?;
        if needs.len() != expected.len()
            || expected
                .iter()
                .any(|name| !needs.iter().any(|value| value == name))
        {
            return fail("Artifact review must require every independent prerequisite");
        }
    }
    let secrets = ci["jobs"]["distribution"]["secrets"]
        .as_object()
        .ok_or("Named signing inputs missing")?;
    let named = [
        "NUMERA_RELEASE_SIGNING_KEY",
        "NUMERA_CODE_SIGNING_PFX",
        "NUMERA_CODE_SIGNING_PASSWORD",
    ];
    if secrets.len() != named.len()
        || named.iter().any(|name| {
            secrets
                .get(*name)
                .is_none_or(|value| value != &format!("${{{{ secrets.{name} }}}}"))
        })
    {
        return fail("Signed builds must receive only their explicitly named signing inputs");
    }
    let steps = release["jobs"]["review"]["steps"]
        .as_array()
        .ok_or("Independent release review missing")?;
    let checkouts: Vec<_> = steps
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|name| name.starts_with("actions/checkout@"))
        })
        .collect();
    if checkouts.len() != 1
        || checkouts[0]["with"]["ref"] != "${{ github.sha }}"
        || checkouts[0]["with"]["persist-credentials"] != false
    {
        return fail(
            "Artifact review must directly checkout its trusted run source without persisted credentials",
        );
    }
    let codeql = steps.iter().position(|step| {
        step["run"]
            .as_str()
            .is_some_and(|run| run == "cargo xtask ci codeql-verdict")
    });
    let verification = steps.iter().position(|step| step["run"].as_str().is_some_and(|run| run == "cargo xtask ci signed-product-review build/owner-review \"${{ inputs.source_sha }}\""));
    if !matches!((codeql, verification), (Some(codeql), Some(verification)) if codeql < verification)
    {
        return fail(
            "Signed artifact review requires independent exact-source CodeQL before signature and identity verification",
        );
    }
    let text = workflows.to_string();
    for required in [
        "workflow_dispatch",
        "KiaroSama",
        "inputs.trusted_sha == github.sha",
        "needs.attest.result == 'success'",
        "refs/heads/main",
        "codeql",
    ] {
        if !text.contains(required) {
            return fail(format!("Manual artifact review policy missing {required}"));
        }
    }
    fn authority(value: &Value) -> bool {
        match value {
            Value::Object(fields) => {
                if fields.contains_key("continue-on-error")
                    || fields.get("secrets").is_some_and(|v| v == "inherit")
                    || fields.get("permissions").is_some_and(|p| {
                        p == "write-all" || p["contents"] == "write" || p["actions"] == "write"
                    })
                {
                    return false;
                }
                fields.values().all(authority)
            }
            Value::Array(values) => values.iter().all(authority),
            Value::String(command) => ![
                "git push",
                "gh release create",
                "gh release upload",
                "gh release edit",
                "--method PATCH",
                "--method=PATCH",
                "-X PATCH",
                "force=true",
            ]
            .iter()
            .any(|forbidden| command.contains(forbidden)),
            _ => true,
        }
    }
    if !authority(workflows) {
        return fail(
            "Artifact review must not publish, mutate refs, inherit credentials or soften failures",
        );
    }
    Ok(())
}
