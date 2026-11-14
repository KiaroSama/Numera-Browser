use super::*;
use crate::resources::{FileRecord, SourceOffer};
use std::{
    io::Read,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "numera product tests {} {}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, content: &[u8]) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(path)
    }
    fn git(&self, args: &[&str]) -> Result<(), Box<dyn Error>> {
        if !Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .status()?
            .success()
        {
            return Err("fixture Git operation failed".into());
        }
        Ok(())
    }
    fn commit(&self) -> Result<BuildIdentity, Box<dyn Error>> {
        self.git(&["init", "--quiet"])?;
        self.git(&["add", "."])?;
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ])?;
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.0)
            .output()?;
        BuildIdentity::parse(
            String::from_utf8(head.stdout)?.trim(),
            "42",
            "linux",
            "x86_64",
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity(platform: &str) -> Result<BuildIdentity, Box<dyn Error>> {
    BuildIdentity::parse(&"a".repeat(40), "42", platform, "x86_64")
}

fn resources(fixture: &Fixture, platform: &str) -> Result<Receipt, Box<dyn Error>> {
    let mut records = Vec::new();
    let mut write = |path: &str, content: &[u8]| -> Result<(), Box<dyn Error>> {
        let file = fixture.write(path, content)?;
        records.push(FileRecord {
            path: path.into(),
            size: content.len() as u64,
            sha256: hash(&file)?,
        });
        Ok(())
    };
    let node = if platform == "windows" {
        "node.exe"
    } else {
        "node"
    };
    for backend in ["playwright", "patchright", "camoufox", "cloak"] {
        write(
            &format!("drivers/{backend}/{node}"),
            b"explicitly provisioned node",
        )?;
        write(
            &format!("drivers/{backend}/package/cli.js"),
            b"vendor SDK entry",
        )?;
        write(
            &format!("drivers/{backend}/package/package.json"),
            if backend == "camoufox" {
                br#"{"version":"1.62.0"}"#
            } else {
                br#"{"version":"1.64.0"}"#
            },
        )?;
    }
    let mut browsers = json!({"camoufox":{"addons":[],"version":"135.0"}});
    for backend in ["playwright", "patchright", "camoufox"] {
        let relative = format!("{backend}/browser");
        let content = format!("{backend} provisioned browser bytes");
        write(&format!("browsers/{relative}"), content.as_bytes())?;
        browsers[backend]["sha256"] = hash(&fixture.0.join(format!("browsers/{relative}")))?.into();
        browsers[backend]["executable"] = relative.into();
    }
    write("browsers/manifest.json", &serde_json::to_vec(&browsers)?)?;
    write(
        "notices/VENDOR-LICENSES.txt",
        b"full vendor licenses fixture",
    )?;
    write(
        "sources/third-party/camoufox-source.zip",
        b"exact corresponding patches/source fixture",
    )?;
    let receipt = Receipt {
        schema: 1,
        platform: platform.into(),
        architecture: "x86_64".into(),
        files: records,
        source_offers: [
            "node",
            "playwright",
            "patchright",
            "camoufox",
            "chromium",
            "firefox",
        ]
        .into_iter()
        .map(|component| SourceOffer {
            component: component.into(),
            version: "1.0.0".into(),
            license: if component == "camoufox" {
                "MPL-2.0".into()
            } else {
                "fixture license".into()
            },
            source_url: format!("https://example.invalid/source/{component}"),
            archive: if component == "camoufox" {
                Some("sources/third-party/camoufox-source.zip".into())
            } else {
                None
            },
        })
        .collect(),
    };
    fixture.write(resources::RECEIPT, &serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}

pub(crate) fn command_requires_explicit_resources_and_native_bundle_targets()
-> Result<(), Box<dyn Error>> {
    for args in [
        vec!["output"],
        vec!["output", "--resources"],
        vec!["output", "--resources", "vendor", "--resources", "other"],
        vec![
            "output",
            "--resources",
            "vendor",
            "--server-only",
            "--bundles",
            "none",
        ],
    ] {
        assert!(Options::parse(args.into_iter().map(OsString::from)).is_err());
    }
    let parsed = Options::parse(
        [
            "path with spaces",
            "--resources",
            "provisioned resources",
            "--server-only",
        ]
        .into_iter()
        .map(OsString::from),
    )?;
    assert_eq!(parsed.output, PathBuf::from("path with spaces"));
    assert!(parsed.server_only);
    for (platform, expected) in [
        ("linux", vec!["appimage", "deb"]),
        ("windows", vec!["msi", "nsis"]),
        ("macos", vec!["app", "dmg"]),
    ] {
        assert_eq!(native_bundles(platform, None)?, expected);
        assert!(native_bundles(platform, Some("none"))?.is_empty());
    }
    for value in ["msi", "appimage,appimage", "deb,", "../deb"] {
        assert!(native_bundles("linux", Some(value)).is_err());
    }
    Ok(())
}

pub(crate) fn product_identity_rejects_candidate_foreign_component_and_wrong_source()
-> Result<(), Box<dyn Error>> {
    let identity = identity("linux")?;
    let info = json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser","version":env!("CARGO_PKG_VERSION"),"source_sha":identity.source_sha,"generation":42,"application_runtime":"native","acceptance":false,"component":"browser-runtime"});
    verify_identity(&identity, &info, "engine")?;
    verify_identity(&identity, &info, "browser-runtime")?;
    for field in [
        "schema",
        "owner",
        "product",
        "version",
        "source_sha",
        "generation",
        "application_runtime",
        "acceptance",
        "component",
    ] {
        let mut changed = info.clone();
        changed[field] = Value::Null;
        assert!(
            verify_identity(&identity, &changed, "browser-runtime").is_err(),
            "accepted missing {field}"
        );
    }
    let mut candidate = info.clone();
    candidate["application_runtime"] = "native-candidate".into();
    candidate["acceptance"] = true.into();
    assert!(verify_identity(&identity, &candidate, "engine").is_err());
    assert!(verify_identity(&identity, &candidate, "desktop").is_err());
    Ok(())
}

pub(crate) fn provisioned_resources_are_exact_hashed_complete_and_platform_specific()
-> Result<(), Box<dyn Error>> {
    explicit_resource_preparation_and_copying_refuse_changed_inputs()?;
    for platform in ["linux", "windows", "macos"] {
        let fixture = Fixture::new()?;
        resources(&fixture, platform)?;
        Receipt::validate(&fixture.0, &identity(platform)?)?;
        assert!(
            Receipt::validate(
                &fixture.0,
                &identity(if platform == "linux" {
                    "windows"
                } else {
                    "linux"
                })?
            )
            .is_err()
        );
        fixture.write("drivers/playwright/package/cli.js", b"changed vendor SDK")?;
        assert!(Receipt::validate(&fixture.0, &identity(platform)?).is_err());
    }
    let fixture = Fixture::new()?;
    resources(&fixture, "linux")?;
    fixture.write("drivers/unlisted.bin", b"unlisted bytes")?;
    assert!(Receipt::validate(&fixture.0, &identity("linux")?).is_err());
    fs::remove_file(fixture.0.join("drivers/unlisted.bin"))?;
    fs::remove_file(fixture.0.join("drivers/camoufox/node"))?;
    assert!(Receipt::validate(&fixture.0, &identity("linux")?).is_err());
    Ok(())
}

fn explicit_resource_preparation_and_copying_refuse_changed_inputs() -> Result<(), Box<dyn Error>> {
    vendor_archive_contracts()?;
    let fixture = Fixture::new()?;
    let receipt = resources(&fixture, env::consts::OS)?;
    fs::remove_file(fixture.0.join(resources::RECEIPT))?;
    let offer_directory = Fixture::new()?;
    let offers = offer_directory.write(
        "source-offers.json",
        &serde_json::to_vec(&receipt.source_offers)?,
    )?;
    resources::prepare(&fixture.0, &offers)?;
    assert!(resources::prepare(&fixture.0, &offers).is_err());
    let missing = Fixture::new()?;
    assert!(resources::prepare(&missing.0, &offers).is_err());
    assert!(!missing.0.join(resources::RECEIPT).exists());
    let destination = Fixture::new()?;
    fixture.write(
        "drivers/playwright/package/cli.js",
        b"vendor bytes changed after validation",
    )?;
    assert!(receipt.copy(&fixture.0, &destination.0).is_err());
    Ok(())
}

fn vendor_archive_contracts() -> Result<(), Box<dyn Error>> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    // Invoke the actual native test harness as a finite vendor command. Its
    // invalid argument must remain visible through the provisioning boundary.
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--numera-vendor-diagnostic-fixture");
    let error = crate::provision::execute(&mut command, 5)
        .err()
        .ok_or("Failed vendor command was accepted")?;
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("exit="));
    assert!(diagnostic.contains("natural_settlement=Some(true)"));
    assert!(diagnostic.contains("numera-vendor-diagnostic-fixture"));
    assert!(diagnostic.contains("Captured stderr:"));
    let synthetic = "q".repeat(1000);
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(format!("--{} token:{synthetic}", "x".repeat(17_000)));
    let error = crate::provision::execute(&mut command, 5)
        .err()
        .ok_or("Failed verbose vendor command was accepted")?;
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("[earlier output omitted]"));
    assert!(diagnostic.contains("[REDACTED]"));
    assert!(!diagnostic.contains(&synthetic));
    assert!(diagnostic.len() < 33_000);

    // A directory link can appear before links inside its target. Publishing
    // that directory too early silently omits required browser resources.
    let fixture = Fixture::new()?;
    let archive = fixture.0.join("vendor.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&archive)?);
    let options = SimpleFileOptions::default();
    writer.start_file("driver/real/bytes", options)?;
    writer.write_all(b"complete vendor resource")?;
    writer.add_symlink("driver/alias", "real", options)?;
    writer.add_symlink("driver/real/dependent", "bytes", options)?;
    writer.finish()?;
    let output = fixture.0.join("unpacked");
    crate::provision::extract(&archive, &output, "driver/")
        .map_err(|error| error as Box<dyn Error>)?;
    for relative in [
        "real/bytes",
        "real/dependent",
        "alias/bytes",
        "alias/dependent",
    ] {
        assert_eq!(
            fs::read(output.join(relative))?,
            b"complete vendor resource",
            "missing materialized member {relative}"
        );
        assert!(fs::symlink_metadata(output.join(relative))?.is_file());
    }
    let retained = output.join("caller-file");
    fs::write(&retained, b"caller state")?;
    assert!(crate::provision::extract(&archive, &output, "driver/").is_err());
    assert_eq!(fs::read(retained)?, b"caller state");
    fn package_archive(entries: &[(&str, tar::EntryType, &[u8])]) -> io::Result<Vec<u8>> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut archive = tar::Builder::new(gzip);
        for &(name, kind, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_entry_type(kind);
            header.set_size(bytes.len() as u64);
            // Raw names deliberately include traversal controls which tar's
            // safe fixture writer would otherwise reject before our reader.
            assert!(name.len() < 100);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            archive.append(&header, bytes)?;
        }
        archive.into_inner()?.finish()
    }
    let package_bytes = package_archive(&[
        ("package/cli.js", tar::EntryType::Regular, b"vendor entry"),
        (
            "package/package.json",
            tar::EntryType::Regular,
            br#"{"name":"playwright-core","version":"1.65.1"}"#,
        ),
    ])?;
    let archive = fixture.write("package.tgz", &package_bytes)?;
    let unpacked = fixture.0.join("driver-package/package");
    crate::provision::extract(&archive, &unpacked, "package/")
        .map_err(|error| error as Box<dyn Error>)?;
    assert_eq!(fs::read(unpacked.join("cli.js"))?, b"vendor entry");
    assert!(crate::provision::extract(&archive, &unpacked, "package/").is_err());
    let driver = unpacked.parent().ok_or("Fixture driver parent missing")?;
    crate::provision::verify_driver(driver, "1.65.1", "playwright")
        .map_err(|error| error as Box<dyn Error>)?;
    for manifest in [
        json!({"name":"playwright-core","version":"1.65.0"}),
        json!({"name":"patchright-core","version":"1.65.1"}),
        json!({"name":"playwright-core","version":null}),
    ] {
        fs::write(
            unpacked.join("package.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        assert!(crate::provision::verify_driver(driver, "1.65.1", "playwright").is_err());
    }
    for (index, entries) in [
        vec![("outside/cli.js", tar::EntryType::Regular, &b"x"[..])],
        vec![("package/../escape", tar::EntryType::Regular, &b"x"[..])],
        vec![("package//absolute", tar::EntryType::Regular, &b"x"[..])],
        vec![("package/C:/escape", tar::EntryType::Regular, &b"x"[..])],
        vec![("package/a\\b", tar::EntryType::Regular, &b"x"[..])],
        vec![("package/link", tar::EntryType::Symlink, &b""[..])],
        vec![("package/link", tar::EntryType::Link, &b""[..])],
        vec![("package/fifo", tar::EntryType::Fifo, &b""[..])],
        vec![("package/", tar::EntryType::Directory, &b"x"[..])],
        vec![
            (
                "package/metadata",
                tar::EntryType::new(b'L'),
                &b"package/cli.js\0"[..],
            ),
            ("package/placeholder", tar::EntryType::Regular, &b"x"[..]),
        ],
        vec![
            (
                "package/metadata",
                tar::EntryType::new(b'x'),
                &b"23 path=package/cli.js\n"[..],
            ),
            ("package/placeholder", tar::EntryType::Regular, &b"x"[..]),
        ],
        vec![
            ("package/a", tar::EntryType::Regular, &b"a"[..]),
            ("package/a", tar::EntryType::Regular, &b"b"[..]),
        ],
        vec![],
    ]
    .into_iter()
    .enumerate()
    {
        let archive = fixture.write(
            &format!("bad-package-{index}.tgz"),
            &package_archive(&entries)?,
        )?;
        assert!(
            crate::provision::extract(
                &archive,
                &fixture.0.join(format!("bad-package-{index}")),
                "package/"
            )
            .is_err()
        );
    }
    for (name, target) in [("escape", "../outside"), ("cycle", "cycle")] {
        let archive = fixture.0.join(format!("{name}.zip"));
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive)?);
        writer.add_symlink(name, target, options)?;
        writer.finish()?;
        assert!(
            crate::provision::extract(&archive, &fixture.0.join(format!("{name}-output")), "")
                .is_err()
        );
    }
    Ok(())
}

pub(crate) fn resource_paths_licenses_and_cloak_redistribution_fail_closed()
-> Result<(), Box<dyn Error>> {
    for path in [
        "../escape",
        "/absolute",
        "C:/absolute",
        "drivers/../../secret",
        "drivers\\secret",
        "drivers/.env",
        "data/profile.json",
        "drivers/.git/config",
        "drivers/a\nfile",
    ] {
        assert!(
            resources::public_path(path, true).is_err(),
            "accepted {path:?}"
        );
    }
    assert!(
        resources::public_path("drivers/playwright/package/node_modules/vendor.js", true).is_ok()
    );
    assert!(resources::public_path("node_modules/vendor.js", false).is_err());
    let fixture = Fixture::new()?;
    let mut receipt = resources(&fixture, "linux")?;
    receipt.source_offers.clear();
    fixture.write(resources::RECEIPT, &serde_json::to_vec(&receipt)?)?;
    assert!(Receipt::validate(&fixture.0, &identity("linux")?).is_err());
    let mut receipt = resources(&fixture, "linux")?;
    let path = fixture.write("browsers/cloak/browser", b"restricted browser")?;
    receipt.files.push(FileRecord {
        path: "browsers/cloak/browser".into(),
        size: fs::metadata(&path)?.len(),
        sha256: hash(&path)?,
    });
    fixture.write(resources::RECEIPT, &serde_json::to_vec(&receipt)?)?;
    assert!(Receipt::validate(&fixture.0, &identity("linux")?).is_err());
    Ok(())
}

#[cfg(unix)]
pub(crate) fn resource_links_are_rejected_before_copying_even_with_a_matching_receipt()
-> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new()?;
    resources(&fixture, "linux")?;
    fs::remove_file(fixture.0.join("drivers/playwright/node"))?;
    symlink(
        fixture.0.join("drivers/patchright/node"),
        fixture.0.join("drivers/playwright/node"),
    )?;
    assert!(Receipt::validate(&fixture.0, &identity("linux")?).is_err());
    Ok(())
}

pub(crate) fn exact_source_refuses_wrong_sha_dirty_inputs_untracked_files_and_private_paths()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    fixture.write("LICENSE", b"source license")?;
    let identity = fixture.commit()?;
    verify_source(&fixture.0, &identity)?;
    assert!(verify_source(&fixture.0, &super::tests::identity("linux")?).is_err());
    fixture.write("LICENSE", b"changed tracked source")?;
    assert!(verify_source(&fixture.0, &identity).is_err());
    fixture.write("LICENSE", b"source license")?;
    fixture.write("untracked.txt", b"untracked build input")?;
    assert!(verify_source(&fixture.0, &identity).is_err());
    fs::remove_file(fixture.0.join("untracked.txt"))?;
    fixture.write(".env", b"private input")?;
    let private = fixture.commit()?;
    assert!(verify_source(&fixture.0, &private).is_err());
    Ok(())
}

pub(crate) fn dashboard_requires_matching_source_receipt_and_rust_wasm()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    fixture.write("index.html", b"Rust dashboard bootstrap")?;
    fixture.write("assets/app.css", b"style")?;
    fixture.write(
        "ci-source.json",
        &serde_json::to_vec(&json!({"source_sha":"b".repeat(40),"version":env!("CARGO_PKG_VERSION"),"runtime":"rust-wasm32","source_dirty":false}))?,
    )?;
    assert!(verify_dashboard(&fixture.0, &identity("linux")?).is_err());
    fixture.write(
        "ci-source.json",
        &serde_json::to_vec(&json!({"source_sha":"a".repeat(40),"version":env!("CARGO_PKG_VERSION"),"runtime":"rust-wasm32","source_dirty":false}))?,
    )?;
    assert!(verify_dashboard(&fixture.0, &identity("linux")?).is_err());
    fixture.write("assets/numera_dashboard_bg.wasm", b"\0asm fixture")?;
    verify_dashboard(&fixture.0, &identity("linux")?)?;
    Ok(())
}

pub(crate) fn native_layout_archives_exact_binaries_wasm_licenses_sources_and_unsigned_provenance()
-> Result<(), Box<dyn Error>> {
    for platform in ["linux", "windows", "macos"] {
        check_native_layout(platform)?;
    }
    Ok(())
}

fn check_native_layout(platform: &str) -> Result<(), Box<dyn Error>> {
    let root = Fixture::new()?;
    for relative in [
        "LICENSE",
        "docs/licensing.md",
        "docs/browser-notices.txt",
        "docs/migration-random-notices.txt",
        "engine-rust/src/http/documentation-assets/NOTICE.md",
    ] {
        root.write(relative, format!("notice: {relative}").as_bytes())?;
    }
    let mut identity = root.commit()?;
    identity.platform = platform.into();
    let output = Fixture::new()?;
    let source = source_archive(&root.0, &output.0, &identity)?;
    let vendor = Fixture::new()?;
    let receipt = resources(&vendor, platform)?;
    let dashboard = Fixture::new()?;
    dashboard.write("index.html", b"Rust UI")?;
    dashboard.write("assets/ui.wasm", b"exact Rust WASM bytes")?;
    let engine = output.write("compiled engine", b"exact native engine bytes")?;
    let browser = output.write("compiled browser runtime", b"exact native browser bytes")?;
    let target = output.0.join("Numera Browser with spaces");
    let inputs = StageInputs {
        root: &root.0,
        source: &source,
        dashboard: &dashboard.0,
        resources_root: &vendor.0,
        receipt: &receipt,
        engine: &engine,
        browser: &browser,
        identity: &identity,
    };
    stage(&target, &inputs)?;
    let engine_name = if platform == "windows" {
        "numera-engine.exe"
    } else {
        "numera-engine"
    };
    let browser_name = if platform == "windows" {
        "numera-browser-runtime.exe"
    } else {
        "numera-browser-runtime"
    };
    assert_eq!(
        fs::read(target.join("engine").join(engine_name))?,
        fs::read(&engine)?
    );
    assert_eq!(
        fs::read(target.join("engine").join(browser_name))?,
        fs::read(&browser)?
    );
    assert!(target.join("engine/web/assets/ui.wasm").is_file());
    assert!(
        target
            .join("engine/sources")
            .join(source.file_name().ok_or("source name")?)
            .is_file()
    );
    assert!(target.join("engine/docs/browser-notices.txt").is_file());
    let provenance: Value =
        serde_json::from_slice(&fs::read(target.join("engine/PROVENANCE.json"))?)?;
    assert_eq!(provenance["signing"], "unsigned");
    assert_eq!(provenance["release_authorized"], false);
    assert_eq!(provenance["identity"]["source_sha"], identity.source_sha);
    assert_eq!(provenance["engine_sha256"], hash(&engine)?);
    assert!(!target.join("BUILD.sig").exists());
    let packed = output.0.join("product.zip");
    archive(&target, &packed, "Numera Browser")?;
    let mut zip = zip::ZipArchive::new(File::open(&packed)?)?;
    let mut bytes = Vec::new();
    zip.by_name(&format!("Numera Browser/engine/{engine_name}"))?
        .read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"exact native engine bytes");
    let mut source_zip = zip::ZipArchive::new(File::open(&source)?)?;
    let mut license = String::new();
    source_zip
        .by_name("LICENSE")?
        .read_to_string(&mut license)?;
    assert_eq!(license, "notice: LICENSE");
    assert!(archive(&target, &packed, "Numera Browser").is_err());
    assert!(stage(&target, &inputs).is_err());
    assert!(
        build(
            &root.0,
            Options {
                output: target.clone(),
                resources: vendor.0.clone(),
                server_only: true,
                bundles: None
            }
        )
        .is_err()
    );
    assert_eq!(
        fs::read(target.join("engine").join(engine_name))?,
        b"exact native engine bytes"
    );
    Ok(())
}
