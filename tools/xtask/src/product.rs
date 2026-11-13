//! Build a native server and desktop product without invoking Python or npm.
//! These artifacts are deliberately unsigned. Existing signed release/update
//! publication remains the authority; this command neither publishes nor signs.
use crate::{
    artifact_identity::{BuildIdentity, compiler_artifact, hash},
    resources::{self, Receipt},
};
use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use zip::{CompressionMethod, System, ZipWriter, write::SimpleFileOptions};

pub struct Options {
    pub output: PathBuf,
    pub resources: PathBuf,
    pub server_only: bool,
    pub bundles: Option<String>,
}

impl Options {
    pub fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self, Box<dyn Error>> {
        let output = PathBuf::from(args.next().ok_or("new output directory required")?);
        let mut resources = None;
        let mut server_only = false;
        let mut bundles = None;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--resources") if resources.is_none() => {
                    resources = Some(PathBuf::from(
                        args.next()
                            .ok_or("--resources requires an explicitly provisioned directory")?,
                    ))
                }
                Some("--server-only") if !server_only => server_only = true,
                Some("--bundles") if bundles.is_none() => {
                    bundles = Some(
                        args.next()
                            .and_then(|value| value.into_string().ok())
                            .ok_or("--bundles requires comma-separated native targets or none")?,
                    )
                }
                _ => return Err("unknown or duplicate build-product argument".into()),
            }
        }
        if server_only && bundles.is_some() {
            return Err("--bundles cannot be combined with --server-only".into());
        }
        Ok(Self {
            output,
            resources: resources.ok_or(
                "--resources is required; no SDK/browser acquisition occurs during a build",
            )?,
            server_only,
            bundles,
        })
    }
}

pub fn native_bundles(
    platform: &str,
    requested: Option<&str>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let allowed: &[&str] = match platform {
        "linux" => &["appimage", "deb"],
        "windows" => &["msi", "nsis"],
        "macos" => &["app", "dmg"],
        _ => return Err("unsupported native desktop platform".into()),
    };
    let requested = requested.unwrap_or("all");
    if requested == "none" {
        return Ok(Vec::new());
    }
    if requested == "all" {
        return Ok(allowed.iter().map(|target| (*target).into()).collect());
    }
    let mut result = Vec::new();
    for target in requested.split(',') {
        if !allowed.contains(&target) || result.iter().any(|entry| entry == target) {
            return Err(
                "bundle target is unsupported, duplicated or requires another native host".into(),
            );
        }
        result.push(target.to_owned());
    }
    Ok(result)
}

pub fn verify_source(root: &Path, identity: &BuildIdentity) -> Result<(), Box<dyn Error>> {
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
        return Err("native product packaging requires a clean tracked source tree".into());
    }
    let tree = Command::new("git")
        .args(["ls-tree", "-r", "-z", &identity.source_sha])
        .current_dir(root)
        .output()?;
    if !tree.status.success() {
        return Err("could not inventory exact tracked source".into());
    }
    for entry in tree
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = std::str::from_utf8(entry)?;
        let (metadata, path) = entry.split_once('\t').ok_or("invalid Git tree inventory")?;
        resources::public_path(path, false)?;
        if !metadata.starts_with("100644 blob ") && !metadata.starts_with("100755 blob ") {
            return Err(
                "source publication refuses unchecked links, submodules or non-files".into(),
            );
        }
    }
    Ok(())
}

pub fn verify_identity(
    identity: &BuildIdentity,
    info: &Value,
    component: &str,
) -> Result<(), Box<dyn Error>> {
    if info["schema"] != 1
        || info["owner"] != "KiaroSama"
        || info["product"] != "Numera Browser"
        || info["version"] != env!("CARGO_PKG_VERSION")
        || info["source_sha"] != identity.source_sha
        || info["generation"] != identity.generation
    {
        return Err("compiled product identity differs from the packaging job".into());
    }
    if component != "desktop"
        && (info["application_runtime"] != "native" || info["acceptance"] != false)
    {
        return Err("product build refused a candidate/foreign runtime identity".into());
    }
    if component == "browser-runtime" && info["component"] != "browser-runtime" {
        return Err("browser runtime component identity is incorrect".into());
    }
    if component == "desktop" && info.get("acceptance").is_some_and(|value| value != false) {
        return Err("product build refused an acceptance-only desktop".into());
    }
    Ok(())
}

fn inspect(
    identity: &BuildIdentity,
    executable: &Path,
    component: &str,
) -> Result<(), Box<dyn Error>> {
    let output = Command::new(executable)
        .arg("--build-info")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() || output.stdout.len() > 4096 {
        return Err("built executable refused its identity".into());
    }
    verify_identity(
        identity,
        &serde_json::from_slice(&output.stdout)?,
        component,
    )
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

pub fn build(root: &Path, options: Options) -> Result<(), Box<dyn Error>> {
    if options.output.exists() {
        return Err("output directory must not already exist".into());
    }
    let identity = BuildIdentity::from_environment()?;
    verify_source(root, &identity)?;
    let bundles = native_bundles(&identity.platform, options.bundles.as_deref())?;
    let receipt = Receipt::validate(&options.resources, &identity)?;
    if !options.server_only {
        let cli = Command::new(cargo())
            .args(["tauri", "--version"])
            .current_dir(root)
            .output()?;
        if !cli.status.success()
            || !String::from_utf8_lossy(&cli.stdout)
                .split_whitespace()
                .any(|part| part.starts_with("2."))
        {
            return Err("desktop builds require an explicitly installed cargo-tauri 2.x CLI and native platform SDKs; use --server-only to build the server".into());
        }
    }
    let dashboard = Command::new(cargo())
        .args([
            "run",
            "--manifest-path",
            "dashboard/Cargo.toml",
            "--locked",
            "--bin",
            "dashboard-build",
            "--",
            "--release",
        ])
        .current_dir(root)
        .status()?;
    if !dashboard.success() {
        return Err("Rust dashboard/WASM build failed (provision wasm32-unknown-unknown and the locked wasm-bindgen CLI)".into());
    }
    verify_dashboard(&root.join("dashboard/dist"), &identity)?;
    let built = Command::new(cargo())
        .args([
            "build",
            "--manifest-path",
            "engine-rust/Cargo.toml",
            "--locked",
            "--release",
            "--features",
            "native-product,native-browser",
            "--bin",
            "numera-engine",
            "--bin",
            "numera-browser-runtime",
            "--message-format=json",
        ])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()?;
    if !built.status.success() {
        return Err("native product/SDK runtime compilation failed".into());
    }
    let engine = compiler_artifact(&built.stdout, "numera-engine")?;
    let browser = compiler_artifact(&built.stdout, "numera-browser-runtime")?;
    inspect(&identity, &engine, "engine")?;
    inspect(&identity, &browser, "browser-runtime")?;
    // Verify again after compilation; a changed source must not get the old SHA.
    verify_source(root, &identity)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let result = build_inner(
        root, &options, &identity, &receipt, &engine, &browser, &bundles,
    );
    if result.is_err() {
        // Exclusively created here; no existing user output can be removed.
        if let Err(error) = fs::remove_dir_all(&options.output) {
            eprintln!("product build cleanup failed: {error}");
        }
    }
    result?;
    println!(
        "Built unsigned native {} at {}. Release signing and acceptance remain required.",
        if options.server_only {
            "server"
        } else {
            "server and desktop"
        },
        options.output.display()
    );
    Ok(())
}

pub fn verify_dashboard(dashboard: &Path, identity: &BuildIdentity) -> Result<(), Box<dyn Error>> {
    if !dashboard.join("index.html").is_file() || !dashboard.join("assets").is_dir() {
        return Err("Rust dashboard requires index.html and assets".into());
    }
    let provenance: Value = serde_json::from_slice(&fs::read(dashboard.join("ci-source.json"))?)?;
    if provenance["source_sha"] != identity.source_sha
        || provenance["version"] != env!("CARGO_PKG_VERSION")
        || provenance["runtime"] != "rust-wasm32"
        || provenance["source_dirty"] != false
    {
        return Err("Rust dashboard provenance differs from the exact product source".into());
    }
    let wasm = resources::files(&dashboard.join("assets"))?
        .iter()
        .any(|path| {
            path.extension()
                .is_some_and(|extension| extension == "wasm")
        });
    if !wasm {
        return Err("Rust dashboard WASM asset is missing".into());
    }
    Ok(())
}

fn source_archive(
    root: &Path,
    output: &Path,
    identity: &BuildIdentity,
) -> Result<PathBuf, Box<dyn Error>> {
    let path = output.join(format!("numera-source-{}.zip", identity.source_sha));
    let source = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let status = Command::new("git")
        .args(["archive", "--format=zip", &identity.source_sha])
        .current_dir(root)
        .stdout(source)
        .status()?;
    if !status.success() {
        return Err("matching tracked source archive failed".into());
    }
    Ok(path)
}

pub struct StageInputs<'a> {
    pub root: &'a Path,
    pub source: &'a Path,
    pub dashboard: &'a Path,
    pub resources_root: &'a Path,
    pub receipt: &'a Receipt,
    pub engine: &'a Path,
    pub browser: &'a Path,
    pub identity: &'a BuildIdentity,
}

pub fn stage(stage: &Path, inputs: &StageInputs<'_>) -> Result<(), Box<dyn Error>> {
    let StageInputs {
        root,
        source,
        dashboard,
        resources_root,
        receipt,
        engine,
        browser,
        identity,
    } = *inputs;
    fs::create_dir(stage)?;
    let runtime = stage.join("engine");
    let suffix = if identity.platform == "windows" {
        ".exe"
    } else {
        ""
    };
    resources::copy_file(engine, &runtime.join(format!("numera-engine{suffix}")))?;
    resources::copy_file(
        browser,
        &runtime.join(format!("numera-browser-runtime{suffix}")),
    )?;
    resources::copy_tree(dashboard, &runtime.join("web"))?;
    receipt.copy(resources_root, &runtime)?;
    resources::copy_file(
        source,
        &runtime
            .join("sources")
            .join(source.file_name().ok_or("source filename missing")?),
    )?;
    for relative in [
        "LICENSE",
        "docs/licensing.md",
        "docs/browser-notices.txt",
        "docs/migration-random-notices.txt",
        "engine-rust/src/http/documentation-assets/NOTICE.md",
    ] {
        resources::copy_file(&root.join(relative), &runtime.join(relative))?;
    }
    resources::copy_file(&root.join("LICENSE"), &stage.join("LICENSE"))?;
    write_json(
        &runtime.join("PROVENANCE.json"),
        &json!({
            "schema":1, "identity":identity, "application_runtime":"native", "dashboard":"rust-wasm",
            "signing":"unsigned", "release_authorized":false, "migration_acceptance":"required",
            "source_archive":source.file_name().and_then(|name| name.to_str()), "source_sha256":hash(source)?,
            "engine_sha256":hash(engine)?, "browser_runtime_sha256":hash(browser)?,
            "resource_receipt_sha256":hash(&resources_root.join(resources::RECEIPT))?,
            "resources_assurance":"operator-provisioned hash inventory; upstream authenticity is not asserted",
            "source_offers":receipt.source_offers,
            "cloak_binary_included":false,
            "runtime_prerequisites": {
                "windows":"Microsoft WebView2 (desktop)",
                "linux":"compatible glibc; WebKitGTK 4.1 and GTK3 (desktop)",
                "macos":"compatible macOS; application is unsigned and not notarized"
            }
        }),
    )?;
    fs::write(
        runtime.join("README.txt"),
        concat!(
            "Numera Browser native product build\n\n",
            "Run numera-engine serve --host 127.0.0.1 --port 8787 from this directory.\n",
            "The native engine serves its Rust/WASM dashboard from web/. Set NUMERA_DATA_DIR\n",
            "to choose persistent storage. Vendor SDKs and browsers are provisioned resources;\n",
            "the build performs no SDK/browser downloads. Cloak binaries are not redistributed.\n",
            "Provision an appropriately licensed Cloak binary separately when using that backend.\n\n",
            "This artifact is unsigned and is not an authenticated release or update.\n",
            "SHA256SUMS and PROVENANCE.json provide integrity/source receipts, not signatures.\n",
            "Signed release/update publication and full migration acceptance remain required.\n"
        ),
    )?;
    inventory(&runtime)?;
    Ok(())
}

fn build_inner(
    root: &Path,
    options: &Options,
    identity: &BuildIdentity,
    receipt: &Receipt,
    engine: &Path,
    browser: &Path,
    bundles: &[String],
) -> Result<(), Box<dyn Error>> {
    let source = source_archive(root, &options.output, identity)?;
    let stage_path = options.output.join("Numera Browser");
    stage(
        &stage_path,
        &StageInputs {
            root,
            source: &source,
            dashboard: &root.join("dashboard/dist"),
            resources_root: &options.resources,
            receipt,
            engine,
            browser,
            identity,
        },
    )?;
    let server_name = format!(
        "numera-server-{}-{}-{}.zip",
        identity.platform, identity.architecture, identity.source_sha
    );
    archive(
        &stage_path.join("engine"),
        &options.output.join(server_name),
        "Numera Browser",
    )?;
    if !options.server_only {
        build_desktop(root, options, identity, bundles, &stage_path)?;
        inventory(&stage_path)?;
        let desktop_name = format!(
            "numera-desktop-{}-{}-{}.zip",
            identity.platform, identity.architecture, identity.source_sha
        );
        archive(
            &stage_path,
            &options.output.join(desktop_name),
            "Numera Browser",
        )?;
    }
    // Exact build inputs must still match when the source receipt is finalized.
    verify_source(root, identity)?;
    let mut assets = Vec::new();
    for entry in fs::read_dir(&options.output)? {
        let path = entry?.path();
        if path.is_file() {
            assets.push(json!({"name":path.file_name().and_then(|name| name.to_str()), "size":fs::metadata(&path)?.len(), "sha256":hash(&path)?}));
        }
    }
    assets.sort_by_key(|asset| asset["name"].as_str().unwrap_or("").to_owned());
    write_json(
        &options.output.join("distribution.json"),
        &json!({"schema":1, "identity":identity, "application_runtime":"native", "signing":"unsigned", "release_authorized":false, "assets":assets}),
    )?;
    let mut checksums = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(options.output.join("SHA256SUMS"))?;
    for asset in &assets {
        writeln!(
            checksums,
            "{}  {}",
            asset["sha256"].as_str().ok_or("asset digest missing")?,
            asset["name"].as_str().ok_or("asset name missing")?
        )?;
    }
    writeln!(
        checksums,
        "{}  distribution.json",
        hash(&options.output.join("distribution.json"))?
    )?;
    checksums.sync_all()?;
    Ok(())
}

fn build_desktop(
    root: &Path,
    options: &Options,
    identity: &BuildIdentity,
    bundles: &[String],
    stage: &Path,
) -> Result<(), Box<dyn Error>> {
    let work = options.output.join(".desktop-build");
    fs::create_dir(&work)?;
    let work = work.canonicalize()?;
    let config = work.join("tauri.product.json");
    let runtime = stage.join("engine").canonicalize()?;
    let resource_map =
        serde_json::Map::from_iter([(runtime.to_string_lossy().into_owned(), json!("engine/"))]);
    write_json(
        &config,
        &json!({"build":{"frontendDist":root.join("dashboard/dist").canonicalize()?}, "bundle":{"resources":resource_map, "targets":bundles}}),
    )?;
    let mut build = Command::new(cargo());
    build
        .args(["tauri", "build", "--ci", "--config"])
        .arg(&config)
        .current_dir(root.join("desktop"))
        .env("CARGO_TARGET_DIR", work.join("target"));
    // Product artifacts are unsigned. Ambient credentials must not turn an
    // explicitly unsigned build into a signing/notarization operation.
    for secret in [
        "NUMERA_RELEASE_SIGNING_KEY",
        "NUMERA_CODE_SIGNING_PFX",
        "NUMERA_CODE_SIGNING_PASSWORD",
        "TAURI_SIGNING_PRIVATE_KEY",
        "TAURI_SIGNING_PRIVATE_KEY_PATH",
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
        "APPLE_CERTIFICATE",
        "APPLE_CERTIFICATE_PASSWORD",
        "APPLE_ID",
        "APPLE_PASSWORD",
        "APPLE_TEAM_ID",
        "APPLE_API_KEY",
        "APPLE_API_ISSUER",
        "APPLE_API_KEY_PATH",
        "APPLE_SIGNING_IDENTITY",
    ] {
        build.env_remove(secret);
    }
    if bundles.is_empty() {
        build.arg("--no-bundle");
    } else {
        build.arg("--bundles").arg(bundles.join(","));
    }
    if !build.status()?.success() {
        return Err("native Tauri desktop/bundle build failed; install cargo-tauri and platform SDK/bundling prerequisites explicitly".into());
    }
    let shell = work
        .join("target/release")
        .join(if identity.platform == "windows" {
            "numera.exe"
        } else {
            "numera"
        });
    inspect(identity, &shell, "desktop")?;
    resources::copy_file(
        &shell,
        &stage.join(shell.file_name().ok_or("desktop filename missing")?),
    )?;
    let bundle_root = work.join("target/release/bundle");
    for target in bundles {
        let directory = bundle_root.join(match target.as_str() {
            "app" => "macos",
            other => other,
        });
        if !directory.is_dir() {
            return Err(format!("Tauri did not produce the requested {target} artifact").into());
        }
        let entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        let mut found = false;
        for entry in entries {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("bundle filename is not Unicode")?;
            let matches = match target.as_str() {
                "app" => name.ends_with(".app"),
                "dmg" => name.ends_with(".dmg"),
                "deb" => name.ends_with(".deb"),
                "appimage" => name.ends_with(".AppImage"),
                "msi" => name.ends_with(".msi"),
                "nsis" => name.ends_with(".exe"),
                _ => false,
            };
            if !matches {
                continue;
            }
            found = true;
            let destination = options.output.join(format!(
                "numera-{}-{}-{name}",
                identity.platform, identity.architecture
            ));
            if path.is_dir() {
                archive(&path, &destination.with_extension("app.zip"), name)?;
            } else {
                resources::copy_file(&path, &destination)?;
            }
        }
        if !found {
            return Err(format!("Tauri did not emit a completed {target} bundle").into());
        }
    }
    fs::remove_dir_all(&work)?;
    Ok(())
}

pub fn inventory(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut entries = Vec::new();
    for path in resources::files(root)? {
        let name = resources::portable_name(&path)?;
        if name == "FILES.json" {
            continue;
        }
        entries.push(json!({"path":name, "size":fs::metadata(root.join(&path))?.len(), "sha256":hash(&root.join(path))?}));
    }
    write_json(
        &root.join("FILES.json"),
        &json!({"schema":1,"signing":"unsigned","files":entries}),
    )
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub fn archive(root: &Path, output: &Path, prefix: &str) -> Result<(), Box<dyn Error>> {
    resources::public_path(prefix, true)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .system(System::Unix)
        .compression_method(CompressionMethod::Deflated);
    for relative in resources::files(root)? {
        let name = resources::portable_name(&relative)?;
        resources::public_path(&name, true)?;
        let path = root.join(&relative);
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(&path)?.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = path.extension().is_some_and(|extension| extension == "exe");
        archive.start_file(
            format!("{prefix}/{name}"),
            options.unix_permissions(if executable { 0o755 } else { 0o644 }),
        )?;
        std::io::copy(&mut File::open(path)?, &mut archive)?;
    }
    archive.finish()?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[allow(dead_code)] // These helpers are called by the maintained six binary tests.
#[path = "product_tests.rs"]
pub(crate) mod tests;
