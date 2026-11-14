use numera_core::ownership::capture;
use serde_json::Value;
use std::{
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
pub mod accounting;
pub mod browser_benchmark;
pub mod build_artifact;
pub mod camoufox_runtime;
pub mod candidate;
pub mod compiler;
pub mod container;
pub mod coverage_inventory;
pub mod entry_benchmark;
pub mod evidence;
#[path = "../../../security-scan/src/contract.rs"]
pub mod gitleaks_contract;
pub mod history;
pub mod history_compat;
pub mod mapping;
pub mod memory;
pub mod migration;
pub mod owned;
pub mod product_signing;
pub mod reference;
pub mod release;
pub mod release_signing;
pub mod scope;
pub mod version;
pub mod workflow;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(std::io::Error::other(message.into()).into())
}
pub fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn digest(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let size = input.read(&mut bytes)?;
        if size == 0 {
            break;
        }
        digest.update(&bytes[..size]);
        crate::progress::completed("ci-hashed", size);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
pub fn canonical_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn codeql_output(command: &mut Command, deadline: Instant, bytes: usize) -> Result<Vec<u8>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return fail("CodeQL source verification deadline expired");
    }
    let output = capture(command, remaining, remaining, bytes)?;
    if output.exit != 0 {
        return fail("CodeQL authenticated source verification command failed");
    }
    Ok(output.stdout)
}
fn codeql_api(endpoint: &str, deadline: Instant) -> Result<Value> {
    decode(&codeql_output(
        Command::new("gh").args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            endpoint,
        ]),
        deadline,
        4 * 1024 * 1024,
    )?)
}
pub fn codeql_local_source(root: &Path, deadline: Instant) -> Result<evidence::CodeqlLocalSource> {
    let git_text = |args: &[&str]| -> Result<String> {
        Ok(String::from_utf8(codeql_output(
            Command::new("git")
                .env("GIT_NO_REPLACE_OBJECTS", "1")
                .arg("-C")
                .arg(root)
                .args(args),
            deadline,
            4 * 1024 * 1024,
        )?)?
        .trim()
        .into())
    };
    Ok(evidence::CodeqlLocalSource {
        commit: git_text(&["rev-parse", "--verify", "HEAD"])?,
        tree: git_text(&["rev-parse", "--verify", "HEAD^{tree}"])?,
        clean: git_text(&["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty(),
    })
}
fn codeql_pr_source(root: &Path, repository: &str, sha: &str, deadline: Instant) -> Result<Value> {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return fail("CodeQL PR source bridge requires the GitHub Actions context");
    }
    let event_path =
        PathBuf::from(std::env::var_os("GITHUB_EVENT_PATH").ok_or("PR event is missing")?);
    if fs::metadata(&event_path)?.len() > 4 * 1024 * 1024 {
        return fail("CodeQL PR event exceeds its bounded input size");
    }
    let event = decode(&fs::read(event_path)?)?;
    if event["repository"]["full_name"] != repository {
        return fail("CodeQL PR event repository differs from the owning repository");
    }
    let context = evidence::CodeqlPrContext {
        repository: repository.into(),
        event: std::env::var("GITHUB_EVENT_NAME")?,
        reference: std::env::var("GITHUB_REF")?,
        number: event["number"].as_u64().ok_or("PR number is missing")?,
        event_head: event["pull_request"]["head"]["sha"]
            .as_str()
            .ok_or("PR head is missing")?
            .into(),
        event_base: event["pull_request"]["base"]["sha"]
            .as_str()
            .ok_or("PR base is missing")?
            .into(),
        build_sha: sha.into(),
    };
    if !canonical_sha(&context.event_head) || !canonical_sha(&context.event_base) {
        return fail("CodeQL PR event source identifiers are invalid");
    }
    let local = codeql_local_source(root, deadline)?;
    let pull = codeql_api(
        &format!("repos/{repository}/pulls/{}", context.number),
        deadline,
    )?;
    let merge = codeql_api(&format!("repos/{repository}/git/commits/{sha}"), deadline)?;
    let head = codeql_api(
        &format!("repos/{repository}/git/commits/{}", context.event_head),
        deadline,
    )?;
    Ok(serde_json::to_value(evidence::codeql_pr_subject(
        &context, &pull, &merge, &head, &local,
    )?)?)
}
fn retain_codeql_subject(subject: &Value, check: Option<u64>) -> Result<()> {
    let receipt = serde_json::json!({
        "schema":1, "source_subject":subject,
        "trusted_check_id":check,
        "trusted_verdict":if check.is_some() {"successful"} else {"pending_or_unsuccessful"}
    });
    let parent = std::env::var_os("RUNNER_TEMP")
        .map(PathBuf::from)
        .ok_or("CodeQL subject receipt requires the runner temporary directory")?
        .join("numera-codeql-verdict");
    fs::create_dir_all(&parent)?;
    fs::write(
        parent.join("source-subject.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!(
        "CodeQL source subject: {}",
        serde_json::to_string(&receipt)?
    );
    Ok(())
}
pub fn capture(
    command: &mut Command,
    wall: Duration,
    idle: Duration,
    bytes: usize,
) -> Result<capture::Output> {
    let result = capture::run_observed_streamed(
        command,
        capture::Limits {
            wall,
            idle,
            bytes,
            cleanup: Duration::from_secs(10),
        },
        || false,
        |_, _| {},
        |stream, bytes| {
            crate::progress::completed(
                if stream == 0 {
                    "child-stdout"
                } else {
                    "child-stderr"
                },
                bytes.len(),
            )
        },
    );
    match result {
        Ok(output) => Ok(output),
        Err(mut error) => {
            error.settle(Duration::from_secs(10))?;
            Err(error.into())
        }
    }
}
pub fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let result = capture(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_NO_REPLACE_OBJECTS", "1"),
        Duration::from_secs(30),
        Duration::from_secs(30),
        32 * 1024 * 1024,
    )?;
    if result.exit != 0 {
        return fail("Git source evidence command failed");
    }
    Ok(result.stdout)
}
pub fn regular(path: &Path, directory: bool) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = metadata.file_type().is_symlink();
    if linked || (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return fail("File ownership is uncertain");
    }
    Ok(metadata)
}
pub fn decode(bytes: &[u8]) -> Result<Value> {
    use serde::Deserialize;
    let mut input = serde_json::Deserializer::from_slice(bytes);
    // The first pass checks duplicate keys recursively. A separate Value pass
    // retains serde_json's arbitrary-precision number representation, which
    // is shared with the core crate and uses an internal map deserializer.
    let _checked = Strict::deserialize(&mut input)?;
    input.end()?;
    Ok(serde_json::from_slice(bytes)?)
}
struct Strict(Value);
impl<'de> serde::Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(input: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Strict;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("strict JSON without duplicate fields")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Strict, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| Strict(number.into()))
                    .ok_or_else(|| E::custom("Invalid compiler number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Strict, E> {
                Ok(Strict(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<Strict>()? {
                    values.push(value.0);
                }
                Ok(Strict(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, value)) = map.next_entry::<String, Strict>()? {
                    if values.insert(key, value.0).is_some() {
                        return Err(serde::de::Error::custom("Duplicate compiler field"));
                    }
                }
                Ok(Strict(values.into()))
            }
        }
        input.deserialize_any(Visitor)
    }
}
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("Repository root")
}

pub fn run(root: &Path, mut args: impl Iterator<Item = OsString>) -> Result<()> {
    let command = args
        .next()
        .ok_or("CI operation required")?
        .into_string()
        .map_err(|_| "CI operation must be Unicode")?;
    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "acceptance" => crate::acceptance::command(&rest),
        "release" => release_signing::run(root, &rest),
        "product-sign" => {
            if rest.len() != 2 {
                return fail("product-sign requires product output and platform");
            }
            println!(
                "{}",
                product_signing::sign(
                    root,
                    Path::new(&rest[0]),
                    rest[1].to_str().ok_or("Platform is not UTF8")?
                )?
                .display()
            );
            Ok(())
        }
        "signed-product-review" => {
            if rest.len() != 2 {
                return fail("signed-product-review requires merged artifacts and exact source");
            }
            product_signing::verify(
                root,
                Path::new(&rest[0]),
                rest[1].to_str().ok_or("Source is not UTF8")?,
            )
        }
        "coverage" => workflow::coverage(root),
        "product-compile" => {
            if !rest.is_empty() {
                return fail("product-compile takes no arguments");
            }
            println!("{}", build_artifact::product(root)?.display());
            Ok(())
        }
        "dashboard-identity" => {
            let identity = crate::artifact_identity::BuildIdentity::from_environment()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            crate::product::verify_dashboard(&one_path(&rest)?, &identity)
                .map_err(|error| std::io::Error::other(error.to_string()).into())
        }
        "browser-sample" => browser_benchmark::sample_command(&rest),
        "entry-sample" => entry_benchmark::cli_sample(&rest),
        "entry-http-sample" => entry_benchmark::http_sample(&rest),
        "entry-benchmark" => {
            if rest.len() != 4 {
                return fail("entry-benchmark requires reference,product,worker,destination");
            }
            entry_benchmark::run(
                root,
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                Path::new(&rest[2]),
                Path::new(&rest[3]),
            )
        }
        "browser-benchmark" => {
            if rest.len() != 5 {
                return fail(
                    "browser-benchmark requires reference,product,worker,destination,engine",
                );
            }
            browser_benchmark::run(
                root,
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                Path::new(&rest[2]),
                Path::new(&rest[3]),
                rest[4].to_str().ok_or("Browser engine must be UTF8")?,
            )
        }
        "reference-prepare" => {
            if rest.len() != 2 {
                return fail(
                    "reference-prepare requires destination and current native SDK binary",
                );
            }
            println!(
                "{}",
                reference::prepare(root, Path::new(&rest[0]), Path::new(&rest[1]))?.display()
            );
            Ok(())
        }
        "reference-build" => {
            if rest.len() != 2 {
                return fail("reference-build requires checkpoint directory and receipt");
            }
            let receipt = decode(&fs::read(&rest[1])?)?;
            println!(
                "{}",
                reference::compile(root, Path::new(&rest[0]), &receipt)?.display()
            );
            Ok(())
        }
        "candidate-diagnostics" => {
            let path = one_path(&rest)?;
            for line in candidate::diagnostics(&path)? {
                println!("{line}");
            }
            Ok(())
        }
        "version" => version::verify(root),
        "required" => {
            workflow::successful_needs(&decode(std::env::var("NUMERA_NEEDS_JSON")?.as_bytes())?)
        }
        "codeql-verdict" => {
            let repository = std::env::var("GITHUB_REPOSITORY")?;
            let sha = std::env::var("GITHUB_SHA")?;
            if repository != "KiaroSama/Numera-Browser" || !canonical_sha(&sha) {
                return fail("CodeQL gate requires owning repository and exact source");
            }
            let deadline = Instant::now() + Duration::from_secs(45);
            let subject = if std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok("pull_request") {
                Some(codeql_pr_source(root, &repository, &sha, deadline)?)
            } else {
                None
            };
            let analysis_sha = subject
                .as_ref()
                .and_then(|subject| subject["analysis_commit"].as_str())
                .unwrap_or(&sha);
            if let Some(subject) = &subject {
                retain_codeql_subject(subject, None)?;
            }
            let output = codeql_output(
                Command::new("gh").args([
                    "api",
                    "--hostname",
                    "github.com",
                    "--paginate",
                    "--slurp",
                    &format!(
                        "repos/{repository}/commits/{analysis_sha}/check-runs?per_page=100&filter=latest"
                    ),
                ]),
                deadline,
                4 * 1024 * 1024,
            )?;
            let check = evidence::codeql(&decode(&output)?, analysis_sha)?;
            if let Some(subject) = &subject {
                retain_codeql_subject(subject, Some(check))?;
            }
            println!(
                "Independent CodeQL verdict verified check={check} build_sha={sha} analysis_sha={analysis_sha}"
            );
            Ok(())
        }
        "stage-tracked" => {
            let output = one_path(&rest)?;
            scope::stage_tracked(root, &output)
                .map(|count| println!("Staged {count} exact tracked files"))
        }
        "secret-config" => {
            if rest.len() != 2 {
                return fail("secret-config requires reviewed base and new destination");
            }
            scope::prepare_secret_config(root, Path::new(&rest[0]), Path::new(&rest[1]))
        }
        "signing-history" => {
            if !rest.is_empty() {
                return fail("signing-history accepts no arguments");
            }
            scope::history_signing_material(root)
                .map(|count| println!("Validated {count} pinned historical signing-material blobs"))
        }
        "native-phase" => {
            if rest.len() != 1 {
                return fail("native-phase requires build or test");
            }
            scope::native_phase(root, &rest[0].to_string_lossy())
        }
        "seam" => {
            if rest.len() != 2 {
                return fail("seam requires scope and build/test phase");
            }
            scope::seam(
                root,
                rest[0].to_str().ok_or("Scope must be UTF8")?,
                rest[1].to_str().ok_or("Phase must be UTF8")?,
            )
        }
        "owned" => owned(root, &rest),
        "source-run" => scope::source_run(root, &rest),
        "source-checkpoint" => scope::source_checkpoint(root, &rest),
        "settled-index" => {
            if rest.len() != 2 {
                return fail("settled-index requires checkpoint index");
            }
            let state = scope::read_checkpoint(Path::new(&rest[0]))?;
            let index = rest[1].to_string_lossy().parse::<usize>()?;
            state.module_cursor(index, true)?;
            println!("{}", state.slot_results[index]);
            Ok(())
        }
        "report" => {
            let path = one_path(&rest)?;
            scope::verify_report(root, &decode(&fs::read(path)?)?)
                .map(|count| println!("Verified {count} native source cases"))
        }
        "desktop-history" => history::desktop(root),
        "native-history" => history::native(root),
        "macos-ownership" => {
            if rest.len() != 1 {
                return fail("macos-ownership requires build or test");
            }
            history::macos(root, &rest[0].to_string_lossy())
        }
        "source-inventory" => {
            let value = scope::inventory(root)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        "source-map" => mapping::write(root),
        "source-map-verify" => mapping::write_verified(root, &one_path(&rest)?),
        "capability-inventory" => {
            println!("{}", migration::frozen_native(root)?);
            Ok(())
        }
        "capability-coverage" => {
            let path = one_path(&rest)?;
            let source = migration::frozen_native(root)?;
            if !migration::coverage(root, &source, &decode(&fs::read(path)?)?)?.is_empty() {
                return fail("Native occurrence coverage is incomplete");
            }
            Ok(())
        }
        "container-browser" => {
            if rest.len() != 2 {
                return fail("Container smoke requires image and exact Playwright release");
            }
            container::smoke(root, &rest[0].to_string_lossy(), &rest[1].to_string_lossy())
        }
        "container-contract" => container::definition(root),
        "camoufox-runtime" => {
            if rest.len() != 1 {
                return fail("camoufox-runtime requires exact provisioned resources");
            }
            camoufox_runtime::run(root, Path::new(&rest[0]))
        }
        "container-api" => {
            if rest.len() != 1 {
                return fail("container-api requires the exact built image");
            }
            container::api(&rest[0].to_string_lossy())
        }
        "container-context" => {
            container::context(root, &fs::read_to_string(root.join("Dockerfile"))?)
        }
        "prepare-live" => {
            if rest.len() != 6 {
                return fail(
                    "prepare-live requires worker engine resources backend data environment-file",
                );
            }
            container::prepare_live(
                root,
                Path::new(&rest[0]),
                Path::new(&rest[1]),
                Path::new(&rest[2]),
                &rest[3].to_string_lossy(),
                Path::new(&rest[4]),
                Path::new(&rest[5]),
            )
        }
        "counted-tests" | "counted-wasm-tests" => {
            if rest.len() < 4 || rest[2] != "--" {
                return fail("counted-tests requires expected-count report -- command");
            }
            let wasm = command == "counted-wasm-tests";
            let expected = rest[0].to_string_lossy().parse::<usize>()?;
            let mut command = Command::new(&rest[3]);
            command.args(&rest[4..]).current_dir(root);
            let path = Path::new(&rest[1]);
            let mut partial = [Vec::new(), Vec::new()];
            let output = owned::run_streamed(
                root,
                &mut command,
                owned::limits(
                    Duration::from_secs(900),
                    Duration::from_secs(180),
                    8 * 1024 * 1024,
                ),
                |index, bytes| partial[index].extend_from_slice(bytes),
            );
            let output = match output {
                Ok(output) => output.command,
                Err(error) => {
                    let (stdout, stderr) =
                        evidence::retain_test_diagnostics(path, &partial[0], &partial[1])?;
                    eprintln!(
                        "Native test command failed before a complete verdict: {error}\nCaptured stdout:\n{stdout}\nCaptured stderr:\n{stderr}"
                    );
                    return Err(error);
                }
            };
            let (safe_stdout, safe_stderr) =
                evidence::retain_test_diagnostics(path, &output.stdout, &output.stderr)?;
            let verdict = (|| -> Result<()> {
                if output.exit != 0 {
                    return fail(format!(
                        "Nested test command exited{}; captured diagnostics retained at {}",
                        output.exit,
                        path.display()
                    ));
                }
                scope::counted_tests_format(
                    std::str::from_utf8(&output.stdout)?,
                    expected,
                    output.exit,
                    wasm,
                )?;
                if !wasm {
                    let arguments = rest[3..]
                        .iter()
                        .filter_map(|value| value.to_str())
                        .collect::<Vec<_>>();
                    let target = arguments
                        .windows(2)
                        .find_map(|pair| (pair[0] == "--test").then_some(pair[1]));
                    let manifest = arguments.windows(2).find_map(|pair| {
                        (pair[0] == "--manifest-path").then_some(Path::new(pair[1]))
                    });
                    if let (Some(target), Some(manifest)) = (target, manifest) {
                        let crate_name = if manifest.ends_with("tools/xtask/Cargo.toml") {
                            Some("tools/xtask")
                        } else if manifest.ends_with("engine-rust/Cargo.toml") {
                            Some("engine-rust")
                        } else {
                            None
                        };
                        if let Some(crate_name) = crate_name {
                            let inventory = coverage_inventory::validate(root)?;
                            let available =
                                inventory.expected(crate_name, target, std::env::consts::OS)?;
                            if arguments.contains(&"--exact") {
                                let selected = available
                                    .iter()
                                    .filter(|name| arguments.contains(&name.as_str()))
                                    .cloned()
                                    .collect::<std::collections::BTreeSet<_>>();
                                if selected.len() != 1 {
                                    return fail(
                                        "Exact maintained proof must select one canonical test name",
                                    );
                                }
                                coverage_inventory::validate_selected_output(
                                    &available,
                                    &selected,
                                    std::str::from_utf8(&output.stdout)?,
                                    output.exit,
                                )?;
                            } else {
                                coverage_inventory::validate_expected_output(
                                    &available,
                                    std::str::from_utf8(&output.stdout)?,
                                    output.exit,
                                )?;
                            }
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = verdict {
                eprintln!(
                    "Native test proof refused: {error}\nCaptured stdout:\n{safe_stdout}\nCaptured stderr:\n{safe_stderr}"
                );
                return Err(error);
            }
            println!("Verified exactly{expected} passing native tests; no ignored cases");
            Ok(())
        }
        _ => fail("Unknown native CI operation"),
    }
}
fn owned(root: &Path, args: &[OsString]) -> Result<()> {
    let boundary = args
        .iter()
        .position(|arg| arg == "--")
        .ok_or("owned requires -- before command")?;
    let mut wall = None;
    let mut idle = None;
    let mut index = 0;
    while index < boundary {
        let key = args[index].to_string_lossy();
        if !["--wall", "--idle"].contains(&key.as_ref()) || index + 1 >= boundary {
            return fail("Invalid owned command bounds");
        }
        let value = args[index + 1].to_string_lossy().parse::<f64>()?;
        if !value.is_finite() || value <= 0.0 || value > 3600.0 {
            return fail("Owned command bounds must be finite and positive");
        }
        if key == "--wall" {
            if wall.replace(value).is_some() {
                return fail("Duplicate wall bound");
            }
        } else if idle.replace(value).is_some() {
            return fail("Duplicate idle bound");
        }
        index += 2;
    }
    let (wall, idle) = (
        wall.ok_or("Wall bound required")?,
        idle.ok_or("Idle bound required")?,
    );
    if idle > wall || boundary + 1 >= args.len() {
        return fail("Invalid owned command deadline or executable");
    }
    let mut command = Command::new(&args[boundary + 1]);
    command.args(&args[boundary + 2..]).current_dir(root);
    let mut partial = [Vec::new(), Vec::new()];
    let observed = owned::run_streamed(
        root,
        &mut command,
        owned::limits(
            Duration::from_secs_f64(wall),
            Duration::from_secs_f64(idle),
            16 * 1024 * 1024,
        ),
        |index, bytes| partial[index].extend_from_slice(bytes),
    );
    let observed = match observed {
        Ok(output) => output,
        Err(error) => {
            let path = root.join("build").join(format!(
                "owned-command-failure-{}-{}.log",
                std::process::id(),
                sha(format!("{:?}", std::time::Instant::now()).as_bytes())
            ));
            let (stdout, stderr) =
                evidence::retain_test_diagnostics(&path, &partial[0], &partial[1])?;
            eprintln!(
                "Owned command boundary failed: {error}\nCaptured stdout:\n{stdout}\nCaptured stderr:\n{stderr}\nRetained diagnostic: {}",
                path.display()
            );
            return Err(error);
        }
    };
    let output = observed.command;
    use std::io::Write;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    for line in memory::project(&observed.memory, "native")? {
        eprintln!("{line}");
    }
    if output.exit != 0 {
        return fail(format!("Owned command exited{}", output.exit));
    }
    Ok(())
}
fn one_path(args: &[OsString]) -> Result<PathBuf> {
    if args.len() != 1 {
        return fail("Exactly one path required");
    }
    Ok(PathBuf::from(&args[0]))
}
