use super::{BuildIdentity, Receipt, hash, resources};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
use zip::{CompressionMethod, System, ZipWriter, write::SimpleFileOptions};

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

pub(super) fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
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
        crate::progress::copy(&mut File::open(path)?, &mut archive)?;
    }
    archive.finish()?.sync_all()?;
    Ok(())
}

pub struct StageInputs<'a> {
    pub root: &'a Path,
    pub source: &'a Path,
    pub dashboard: &'a Path,
    pub vendor: Option<(&'a Path, &'a Receipt)>,
    pub engine: &'a Path,
    pub browser: &'a Path,
    pub identity: &'a BuildIdentity,
}

pub fn stage(stage: &Path, inputs: &StageInputs<'_>) -> Result<(), Box<dyn Error>> {
    let StageInputs {
        root,
        source,
        dashboard,
        vendor,
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
    if let Some((resources_root, receipt)) = vendor {
        receipt.copy(resources_root, &runtime)?;
    } else {
        let build = json!({"schema":1,"signing":"unsigned","bundled_browsers":false});
        write_json(&stage.join("BUILD.json"), &build)?;
        write_json(&runtime.join("BUILD.json"), &build)?;
    }
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
            "resource_receipt_sha256":vendor.map(|(path, _)| hash(&path.join(resources::RECEIPT))).transpose()?,
            "resources_assurance":vendor.map(|_| "operator-provisioned hash inventory; upstream authenticity is not asserted"),
            "source_offers":vendor.map_or(&[][..], |(_, receipt)| receipt.source_offers.as_slice()),
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
