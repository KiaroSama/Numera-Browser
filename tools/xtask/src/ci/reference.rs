//! An immutable native checkpoint with only its SDK transport injected.
//! Application logic and dependency lock bytes remain those of the checkpoint.
use super::{Result, fail};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
pub const SOURCE: &str = "3bf1d839fd33e42a10b982bd8607a7052ea7c4cd";
pub const ORIGINAL_BASELINE: &str = "13b6d9c6973281fe123353eaec05b29d65781a2d";
pub const ORIGINAL_TREE: &str = "760f9873ded13c5ebfecae1f16b385e34f695124";
pub const BRIDGE_FILES: [(&str, &str); 2] = [
    (
        "engine-rust/src/browser/runtime.rs",
        "d4e3da6bc8ce2772937bdf0e842d7d5fdae4fa8d",
    ),
    (
        "engine-rust/src/native_environment.rs",
        "f65bd8a6df448e79c0ea1e6b160e4e7fc34fab4f",
    ),
];
fn replace_once(text: &str, from: &str, to: &str) -> Result<String> {
    if text.matches(from).count() != 1 {
        return fail("Native checkpoint transport boundary differs");
    }
    Ok(text.replacen(from, to, 1))
}
pub fn bridge(path: &str, source: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(source)?;
    let text = match path {
        "engine-rust/src/browser/runtime.rs" => {
            let changed = replace_once(
                text,
                "let mut command = Command::new(&self.python);",
                "let mut command = Command::new(&self.adapter);",
            )?;
            replace_once(
                &changed,
                "command\n            .arg(\"-u\")\n            .arg(&self.adapter)\n            .current_dir(&self.cwd)",
                "command\n            .current_dir(&self.cwd)",
            )?
        }
        "engine-rust/src/native_environment.rs" => {
            let start = text
                .find("        let mut command = Command::new(&self.browser.python);")
                .ok_or("Native checkpoint metadata transport missing")?;
            let after = text[start..]
                .find("        let result = capture::run(")
                .ok_or("Native checkpoint metadata capture missing")?
                + start;
            if text[start..after]
                .matches(".env_remove(\"NUMERA_PASSWORD\").env_remove(\"NUMERA_API_TOKEN\");")
                .count()
                != 1
            {
                return fail("Native checkpoint metadata credential boundary differs");
            }
            format!(
                "{}        let mut command = Command::new(&self.browser.adapter);\n        command.arg(\"--metadata\")\n            .env_remove(\"NUMERA_PASSWORD\").env_remove(\"NUMERA_API_TOKEN\");\n{}",
                &text[..start],
                &text[after..]
            )
        }
        _ => return fail("Unreviewed native checkpoint bridge file"),
    };
    Ok(text.into_bytes())
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub blob: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}
pub fn entries(root: &Path) -> Result<Vec<Entry>> {
    let listing = super::git(root, &["ls-tree", "-rz", SOURCE, "--", "engine-rust"])?;
    let mut entries = Vec::new();
    let mut total = 0;
    for row in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row)?;
        let (metadata, path) = row
            .split_once('\t')
            .ok_or("Checkpoint tree row malformed")?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || !matches!(fields[0], "100644" | "100755")
            || fields[1] != "blob"
            || !super::canonical_sha(fields[2])
            || !path.starts_with("engine-rust/")
            || Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || path.contains('\\')
            || path.chars().any(char::is_control)
        {
            return fail("Checkpoint contains an unsafe or nonregular source entry");
        }
        let bytes = super::git(root, &["cat-file", "blob", fields[2]])?;
        total += bytes.len();
        if total > 32 * 1024 * 1024 || entries.len() >= 1024 {
            return fail("Checkpoint source exceeds bounded scope");
        }
        entries.push(Entry {
            path: path.into(),
            blob: fields[2].into(),
            bytes,
            executable: fields[0] == "100755",
        });
    }
    if entries.is_empty() {
        return fail("Checkpoint source inventory empty");
    }
    Ok(entries)
}
pub fn materialize(destination: &Path, entries: &[Entry], worker: &Path) -> Result<Value> {
    if !worker.is_absolute() || !worker.is_file() {
        return fail("Current native SDK binary must be explicit");
    }
    fs::create_dir(destination)?;
    let mut receipt = BTreeMap::new();
    let mut patches = Vec::new();
    for entry in entries {
        let relative = Path::new(&entry.path);
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || !entry.path.starts_with("engine-rust/")
            || receipt.contains_key(&entry.path)
        {
            return fail("Checkpoint inventory path is unsafe or duplicated");
        }
        let bytes = if let Some((_, blob)) =
            BRIDGE_FILES.iter().find(|(path, _)| *path == entry.path)
        {
            if &entry.blob != blob {
                return fail("Checkpoint bridge Git blob differs from reviewed source");
            }
            let changed = bridge(&entry.path, &entry.bytes)?;
            patches.push(json!({"path":entry.path,"git_blob":blob,"before_sha256":super::sha(&entry.bytes),"after_sha256":super::sha(&changed)}));
            changed
        } else {
            entry.bytes.clone()
        };
        let path = destination.join(relative);
        fs::create_dir_all(path.parent().ok_or("Checkpoint parent missing")?)?;
        fs::write(&path, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if entry.executable { 0o755 } else { 0o644 }),
            )?;
        }
        receipt.insert(
            entry.path.clone(),
            json!({"git_blob":entry.blob,"size":bytes.len(),"sha256":super::sha(&bytes)}),
        );
    }
    if patches.len() != 2 {
        return fail("Native checkpoint must inject exactly two reviewed SDK transport sites");
    }
    let patch_sha = super::sha(&serde_json::to_vec(&patches)?);
    Ok(
        json!({"schema":1,"comparison":"native-checkpoint-to-native-product","source_sha":SOURCE,"characterization_provenance":{"source_commit":ORIGINAL_BASELINE,"source_tree":ORIGINAL_TREE,"comparative_baseline":false},"transport_patch_sha256":patch_sha,"sdk_binary_sha256":super::digest(worker)?,"sdk_binary":worker,"bridge":patches,"files":receipt}),
    )
}
pub fn verify(destination: &Path, receipt: &Value) -> Result<()> {
    if receipt["source_sha"] != SOURCE
        || receipt["comparison"] != "native-checkpoint-to-native-product"
    {
        return fail("Checkpoint source identity differs");
    }
    let expected = receipt["files"]
        .as_object()
        .filter(|v| !v.is_empty())
        .ok_or("Checkpoint inventory missing")?;
    fn walk(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, String>,
        depth: usize,
    ) -> Result<()> {
        if depth > 64 || out.len() > 1024 {
            return fail("Checkpoint inventory exceeds its traversal bound");
        }
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            let m = fs::symlink_metadata(&path)?;
            if m.is_symlink() {
                return fail("Checkpoint symbolic link refused");
            }
            if m.is_dir() {
                walk(root, &path, out, depth + 1)?;
            } else if m.is_file() {
                out.insert(
                    path.strip_prefix(root)?
                        .to_str()
                        .ok_or("Checkpoint name is not UTF8")?
                        .replace('\\', "/"),
                    super::digest(&path)?,
                );
            } else {
                return fail("Checkpoint special entry refused");
            }
        }
        Ok(())
    }
    let mut observed = BTreeMap::new();
    walk(destination, destination, &mut observed, 0)?;
    if observed.len() != expected.len() {
        return fail("Checkpoint inventory differs");
    }
    for (path, digest) in observed {
        if expected.get(&path).is_none_or(|row| {
            row["sha256"] != digest
                || row["size"].as_u64()
                    != fs::metadata(destination.join(&path)).ok().map(|m| m.len())
        }) {
            return fail("Checkpoint bytes differ");
        }
    }
    Ok(())
}
pub fn prepare(root: &Path, destination: &Path, worker: &Path) -> Result<PathBuf> {
    let receipt = materialize(destination, &entries(root)?, &worker.canonicalize()?)?;
    verify(destination, &receipt)?;
    let path = destination.with_extension("receipt.json");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()?;
    Ok(path)
}
pub fn compile(root: &Path, destination: &Path, receipt: &Value) -> Result<PathBuf> {
    let destination = destination.canonicalize()?;
    verify(&destination, receipt)?;
    let worker = Path::new(
        receipt["sdk_binary"]
            .as_str()
            .ok_or("Reference SDK binary missing")?,
    );
    if super::digest(worker)? != receipt["sdk_binary_sha256"] {
        return fail("Native reference SDK changed after transport review");
    }
    let mut command = Command::new("cargo");
    // Never admit a caller's shared product target into this historical build.
    let target = destination.with_extension("target");
    command
        .current_dir(destination)
        .args([
            "build",
            "--manifest-path",
            "engine-rust/Cargo.toml",
            "--locked",
            "--release",
            "--no-default-features",
            "--features",
            "native-candidate",
            "--bin",
            "numera-native-candidate",
            "--message-format=json-render-diagnostics",
        ])
        .env("GITHUB_SHA", SOURCE)
        .env("CARGO_TARGET_DIR", &target)
        .env("PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD", "1");
    let output = super::owned::run(
        root,
        &mut command,
        super::owned::limits(
            Duration::from_secs(900),
            Duration::from_secs(300),
            32 * 1024 * 1024,
        ),
    )?;
    if output.command.exit != 0 {
        return fail("Native checkpoint compilation failed");
    }
    crate::artifact_identity::compiler_artifact(&output.command.stdout, "numera-native-candidate")
        .map_err(|error| std::io::Error::other(error.to_string()).into())
}
