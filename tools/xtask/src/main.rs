//! Rust build/packaging commands. Candidate artifacts do not select a runtime.
mod package;

use serde_json::Value;
use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("xtask repository root must exist")
}

fn main() {
    if let Err(error) = run() {
        eprintln!("numera-xtask: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("build-candidate") => {
            let output = args.next().ok_or("output directory required")?;
            if args.next().is_some() {
                return Err("unexpected build argument".into());
            }
            build_candidate(&root(), Path::new(&output))
        }
        _ => Err("usage: cargo xtask build-candidate <new-output-directory>".into()),
    }
}

fn build_candidate(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    // Do not overwrite an earlier build or silently incorporate untracked files.
    if output.exists() {
        return Err("output directory must not already exist".into());
    }
    let identity = package::BuildIdentity::from_environment()?;
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()?;
    if !head.status.success() || String::from_utf8(head.stdout)?.trim() != identity.source_sha {
        return Err("build source differs from the requested source SHA".into());
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .current_dir(root)
        .output()?;
    if !dirty.status.success() || !dirty.stdout.is_empty() {
        return Err("candidate packaging requires a clean tracked source tree".into());
    }
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let built = Command::new(cargo)
        .args([
            "build",
            "--manifest-path",
            "engine-rust/Cargo.toml",
            "--locked",
            "--release",
            "--features",
            "native-candidate,native-browser",
            "--bin",
            "numera-native-candidate",
            "--bin",
            "numera-browser-runtime",
            "--message-format=json",
        ])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()?;
    if !built.status.success() {
        return Err("native candidate compilation failed".into());
    }
    let executable = compiler_artifact(&built.stdout, "numera-native-candidate")?;
    let browser = compiler_artifact(&built.stdout, "numera-browser-runtime")?;
    let reported = Command::new(&executable).arg("--build-info").output()?;
    if !reported.status.success() {
        return Err("built candidate refused its identity".into());
    }
    let info: Value = serde_json::from_slice(&reported.stdout)?;
    identity.verify(&info)?;
    let reported = Command::new(&browser).arg("--build-info").output()?;
    if !reported.status.success() {
        return Err("built browser runtime refused its identity".into());
    }
    let info: Value = serde_json::from_slice(&reported.stdout)?;
    identity.verify(&info)?;
    if info["component"] != "browser-runtime" {
        return Err("browser runtime component is incorrect".into());
    }
    package::write(root, output, &executable, &browser, identity)?;
    println!(
        "Built and packaged native acceptance candidate at {}",
        output.display()
    );
    Ok(())
}

fn compiler_artifact(messages: &[u8], name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut artifacts = Vec::new();
    let mut finished = None;
    for line in messages
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line)?;
        match value["reason"].as_str() {
            Some("compiler-artifact")
                if value["target"]["name"] == name
                    && value["target"]["kind"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|k| k == "bin")) =>
            {
                if let Some(path) = value["executable"].as_str() {
                    artifacts.push(PathBuf::from(path));
                }
            }
            Some("build-finished") => {
                if finished.is_some() {
                    return Err("duplicate Cargo completion record".into());
                }
                finished = value["success"].as_bool();
            }
            _ => {}
        }
    }
    if finished != Some(true) || artifacts.len() != 1 || !artifacts[0].is_file() {
        return Err(
            format!("Cargo did not produce exactly one successful {name} executable").into(),
        );
    }
    Ok(artifacts.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_failed_or_ambiguous_compiler_evidence() {
        for messages in [b"".as_slice(), b"{\"reason\":\"build-finished\",\"success\":false}",
            b"{\"reason\":\"build-finished\",\"success\":true}",
            b"{\"reason\":\"build-finished\",\"success\":true}\n{\"reason\":\"build-finished\",\"success\":true}",
            b"not json"] {
            assert!(compiler_artifact(messages, "numera-native-candidate").is_err());
        }
    }

    #[test]
    fn selects_actual_binary_record_instead_of_an_example_or_cached_path()
    -> Result<(), Box<dyn Error>> {
        let executable = env::current_exe()?;
        let artifact = serde_json::json!({"reason":"compiler-artifact", "target":{"name":"numera-native-candidate","kind":["bin"]}, "executable":executable});
        let finished = serde_json::json!({"reason":"build-finished","success":true});
        let messages = format!("{artifact}\n{finished}\n");
        assert_eq!(
            compiler_artifact(messages.as_bytes(), "numera-native-candidate")?,
            executable
        );
        assert!(compiler_artifact(messages.as_bytes(), "numera-browser-runtime").is_err());
        let mut browser = artifact.clone();
        browser["target"]["name"] = "numera-browser-runtime".into();
        let combined = format!("{artifact}\n{browser}\n{finished}\n");
        assert_eq!(
            compiler_artifact(combined.as_bytes(), "numera-browser-runtime")?,
            executable
        );
        assert_eq!(
            compiler_artifact(combined.as_bytes(), "numera-native-candidate")?,
            executable
        );
        assert!(
            compiler_artifact(
                format!("{artifact}\n{artifact}\n{finished}").as_bytes(),
                "numera-native-candidate"
            )
            .is_err()
        );
        let mut example = artifact.clone();
        example["target"]["kind"] = serde_json::json!(["example"]);
        assert!(
            compiler_artifact(
                format!("{example}\n{finished}").as_bytes(),
                "numera-native-candidate"
            )
            .is_err()
        );
        Ok(())
    }
}
