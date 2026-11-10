use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::Command,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildIdentity {
    pub source_sha: String,
    pub generation: u64,
    pub platform: String,
    pub architecture: String,
}

impl BuildIdentity {
    pub fn from_environment() -> Result<Self, Box<dyn Error>> {
        Self::parse(
            &env::var("GITHUB_SHA")?,
            &env::var("GITHUB_RUN_ID")?,
            env::consts::OS,
            env::consts::ARCH,
        )
    }

    fn parse(source: &str, generation: &str, os: &str, arch: &str) -> Result<Self, Box<dyn Error>> {
        if source.len() != 40
            || !source
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || generation.is_empty()
            || generation.len() > 16
            || generation.starts_with('0')
            || !generation.bytes().all(|b| b.is_ascii_digit())
            || !["linux", "windows", "macos"].contains(&os)
            || !["x86_64", "aarch64"].contains(&arch)
        {
            return Err("invalid candidate build identity".into());
        }
        Ok(Self {
            source_sha: source.into(),
            generation: generation.parse()?,
            platform: os.into(),
            architecture: arch.into(),
        })
    }

    pub fn verify(&self, info: &Value) -> Result<(), Box<dyn Error>> {
        if info["schema"] != 1
            || info["owner"] != "KiaroSama"
            || info["product"] != "Numera Browser"
            || info["version"] != env!("CARGO_PKG_VERSION")
            || info["source_sha"] != self.source_sha
            || info["generation"] != self.generation
            || info["application_runtime"] != "native-candidate"
            || info["acceptance"] != true
        {
            return Err("compiled candidate identity differs from the packaging job".into());
        }
        Ok(())
    }
}

fn hash(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut source = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let read = source.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        digest.update(&bytes[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

const NOTICE: &str = "Numera Browser native acceptance candidate\n\n\
This archive is a CLI/HTTP/MCP acceptance executable, not a complete desktop\n\
distribution or an accepted Rust-only runtime. Browser operations still require\n\
the SDK adapter and its external interpreter/SDK/browser prerequisites. The web\n\
dashboard is not included. Do not replace a production installation with this\n\
candidate. Passing compilation does not certify migration or release acceptance.\n\n\
The matching tracked source archive and SHA256SUMS accompany this archive.\n\
See docs/rust-rewrite.md in that source for the remaining migration work.\n";

pub fn write(
    root: &Path,
    output: &Path,
    executable: &Path,
    identity: BuildIdentity,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let result = write_inner(root, output, executable, &identity);
    if result.is_err() {
        // This directory was exclusively created above; never remove an existing output.
        if let Err(error) = fs::remove_dir_all(output) {
            eprintln!("candidate packaging cleanup failed: {error}");
        }
    }
    result
}

fn write_inner(
    root: &Path,
    output: &Path,
    executable: &Path,
    identity: &BuildIdentity,
) -> Result<(), Box<dyn Error>> {
    let stem = format!(
        "numera-native-candidate-{}-{}",
        identity.platform, identity.architecture
    );
    let archive_name = format!("{stem}.zip");
    let binary_name = if identity.platform == "windows" {
        "numera-native-candidate.exe"
    } else {
        "numera-native-candidate"
    };
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(&archive_name))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file(binary_name, options.unix_permissions(0o755))?;
    std::io::copy(&mut File::open(executable)?, &mut archive)?;
    for (path, name) in [
        ("LICENSE", "LICENSE"),
        ("docs/browser-notices.txt", "browser-notices.txt"),
        (
            "docs/migration-random-notices.txt",
            "migration-random-notices.txt",
        ),
    ] {
        archive.start_file(name, options.unix_permissions(0o644))?;
        std::io::copy(&mut File::open(root.join(path))?, &mut archive)?;
    }
    archive.start_file("README.txt", options.unix_permissions(0o644))?;
    archive.write_all(NOTICE.as_bytes())?;
    archive.start_file("build.json", options.unix_permissions(0o644))?;
    serde_json::to_writer_pretty(
        &mut archive,
        &serde_json::json!({
            "schema":1, "identity":identity, "binary_sha256":hash(executable)?,
            "application_runtime":"native-candidate", "migration_complete":false,
            "browser_adapter_included":false, "dashboard_included":false
        }),
    )?;
    archive.finish()?.sync_all()?;

    let source_name = format!("numera-source-{}.zip", identity.source_sha);
    let source = output.canonicalize()?.join(&source_name);
    let status = Command::new("git")
        .args(["archive", "--format=zip", "--output"])
        .arg(&source)
        .arg(&identity.source_sha)
        .current_dir(root)
        .status()?;
    if !status.success() {
        return Err("matching tracked source archive failed".into());
    }
    let mut checksums = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("SHA256SUMS"))?;
    for name in [archive_name, source_name] {
        writeln!(checksums, "{}  {name}", hash(&output.join(&name))?)?;
    }
    checksums.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_canonical_source_generation_and_supported_platforms() -> Result<(), Box<dyn Error>>
    {
        let sha = "a".repeat(40);
        for os in ["linux", "windows", "macos"] {
            for arch in ["x86_64", "aarch64"] {
                assert_eq!(BuildIdentity::parse(&sha, "42", os, arch)?.generation, 42);
            }
        }
        for generation in ["", "0", "042", "-1", "42\n", "10000000000000000"] {
            assert!(BuildIdentity::parse(&sha, generation, "linux", "x86_64").is_err());
        }
        for source in ["unknown", &"A".repeat(40), &"a".repeat(39)] {
            assert!(BuildIdentity::parse(source, "42", "linux", "x86_64").is_err());
        }
        assert!(BuildIdentity::parse(&sha, "42", "freebsd", "x86_64").is_err());
        assert!(BuildIdentity::parse(&sha, "42", "linux", "unknown").is_err());
        Ok(())
    }

    #[test]
    fn mismatched_candidate_metadata_refuses_packaging() -> Result<(), Box<dyn Error>> {
        let identity = BuildIdentity::parse(&"a".repeat(40), "42", "linux", "x86_64")?;
        let info = serde_json::json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser",
            "version":env!("CARGO_PKG_VERSION"),"source_sha":identity.source_sha,"generation":42,
            "application_runtime":"native-candidate","acceptance":true});
        identity.verify(&info)?;
        for field in [
            "schema",
            "owner",
            "product",
            "version",
            "source_sha",
            "generation",
            "application_runtime",
            "acceptance",
        ] {
            let mut changed = info.clone();
            changed[field] = Value::Null;
            assert!(
                identity.verify(&changed).is_err(),
                "accepted missing {field}"
            );
        }
        Ok(())
    }

    #[test]
    fn archives_exact_source_bytes_and_refuses_existing_outputs() -> Result<(), Box<dyn Error>> {
        let temporary = TestRoot::new()?;
        let root = &temporary.0;
        fs::create_dir(root.join("docs"))?;
        for path in [
            "LICENSE",
            "docs/browser-notices.txt",
            "docs/migration-random-notices.txt",
        ] {
            fs::write(root.join(path), format!("fixture notice: {path}\n"))?;
        }
        for args in [
            vec!["init", "--quiet"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()?
                    .success()
            );
        }
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()?;
        assert!(head.status.success());
        let sha = String::from_utf8(head.stdout)?.trim().to_owned();
        let executable = root.join("untracked-executable");
        fs::write(&executable, b"exact executable bytes")?;
        for os in ["linux", "windows", "macos"] {
            let output = root.join(format!("output-{os}"));
            write(
                root,
                &output,
                &executable,
                BuildIdentity::parse(&sha, "42", os, "x86_64")?,
            )?;
            let mut archive = zip::ZipArchive::new(File::open(
                output.join(format!("numera-native-candidate-{os}-x86_64.zip")),
            )?)?;
            assert_eq!(archive.len(), 6);
            let name = if os == "windows" {
                "numera-native-candidate.exe"
            } else {
                "numera-native-candidate"
            };
            let mut binary = archive.by_name(name)?;
            let mut bytes = Vec::new();
            binary.read_to_end(&mut bytes)?;
            assert_eq!(bytes, b"exact executable bytes");
            assert_eq!(binary.unix_mode().map(|mode| mode & 0o777), Some(0o755));
            drop(binary);
            let mut info = String::new();
            archive.by_name("build.json")?.read_to_string(&mut info)?;
            let info: Value = serde_json::from_str(&info)?;
            assert_eq!(info["identity"]["source_sha"], sha);
            assert_eq!(info["binary_sha256"], hash(&executable)?);
            assert_eq!(info["migration_complete"], false);
            let source =
                zip::ZipArchive::new(File::open(output.join(format!("numera-source-{sha}.zip")))?)?;
            assert_eq!(source.len(), 4); // Three tracked notices and the docs directory.
            let names = source.file_names().collect::<Result<Vec<_>, _>>()?;
            assert!(
                !names
                    .iter()
                    .any(|name| name.contains("untracked-executable") || name.contains("output-"))
            );
            let checksums = fs::read_to_string(output.join("SHA256SUMS"))?;
            for line in checksums.lines() {
                let (digest, path) = line.split_once("  ").ok_or("checksum record missing")?;
                assert_eq!(digest, hash(&output.join(path))?);
            }
            fs::write(output.join("protected"), b"prior build")?;
            assert!(
                write(
                    root,
                    &output,
                    &executable,
                    BuildIdentity::parse(&sha, "42", os, "x86_64")?
                )
                .is_err()
            );
            assert_eq!(fs::read(output.join("protected"))?, b"prior build");
        }
        Ok(())
    }

    #[test]
    fn failed_packaging_removes_only_its_new_directory() -> Result<(), Box<dyn Error>> {
        let temporary = TestRoot::new()?;
        let output = temporary.0.join("failed");
        let executable = temporary.0.join("binary");
        fs::write(&executable, b"fixture")?;
        assert!(
            write(
                &temporary.0,
                &output,
                &executable,
                BuildIdentity::parse(&"a".repeat(40), "42", "linux", "x86_64")?
            )
            .is_err()
        );
        assert!(!output.exists());
        assert_eq!(fs::read(executable)?, b"fixture");
        Ok(())
    }

    struct TestRoot(std::path::PathBuf);
    impl TestRoot {
        fn new() -> std::io::Result<Self> {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/package-tests");
            fs::create_dir_all(&parent)?;
            for _ in 0..32 {
                let path = parent.join(format!(
                    "{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            Err(std::io::Error::other("test namespace exhausted"))
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                eprintln!("packaging fixture cleanup failed: {error}");
            }
        }
    }
}
