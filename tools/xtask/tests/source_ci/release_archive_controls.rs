pub(super) fn ed25519_exact() -> TestResult {
    use ed25519_dalek::{Signature, SigningKey, Verifier};
    let tmp = Temp::new("release-ed25519")?;
    let manifest = tmp.write(
        "update-manifest.json",
        b"{\"schema\":1,\"generation\":42}\n",
    )?;
    let key = SigningKey::from_bytes(&[7; 32]);
    let public = key.verifying_key();
    let signature = tmp.0.join("update-manifest.sig");
    signing::sign_manifest(&manifest, &signature, &key, &public)?;
    let proof = fs::read(&signature)?;
    assert_eq!(proof.len(), 64);
    signing::verify_manifest(&manifest, &signature, &public)?;
    public.verify(&fs::read(&manifest)?, &Signature::from_slice(&proof)?)?;
    assert!(
        signing::sign_manifest(&manifest, &signature, &key, &public)
            .expect_err("exclusive destination")
            .to_string()
            .contains("destination new")
    );
    assert_eq!(fs::read(&signature)?, proof);
    let original = fs::read(&manifest)?;
    fs::write(&manifest, b"{\"schema\":1,\"generation\":43}\n")?;
    assert!(
        signing::verify_manifest(&manifest, &signature, &public)
            .expect_err("tamper")
            .to_string()
            .contains("signature")
    );
    fs::write(&manifest, &original)?;
    let wrong = SigningKey::from_bytes(&[8; 32]);
    let refused = tmp.0.join("wrong-signer.sig");
    assert!(
        signing::sign_manifest(&manifest, &refused, &wrong, &public)
            .expect_err("wrong signer")
            .to_string()
            .contains("pinned public signer")
    );
    assert!(!refused.exists());
    assert_eq!(fs::read(&signature)?, proof);
    assert!(signing::verify_manifest(&manifest, &signature, &wrong.verifying_key()).is_err());
    fs::write(&signature, [b'x'; 63])?;
    assert!(
        signing::verify_manifest(&manifest, &signature, &public)
            .expect_err("wrongsize")
            .to_string()
            .contains("size")
    );
    assert!(
        !fs::read_dir(&tmp.0)?
            .any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with("numera-sign-")))
    );
    Ok(())
}
pub(super) fn inventory(mode: u32) -> TestResult {
    let tmp = Temp::new("release-inventory")?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir(&stage)?;
    let executable = stage.join("numera");
    fs::write(&executable, b"fixture executable, never run")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(mode))?;
    }
    fs::write(stage.join("LICENSE"), b"fixture terms")?;
    fs::create_dir(stage.join(".updates"))?;
    fs::write(stage.join(".updates/runtime.lock"), b"")?;
    fs::write(
        stage.join(".updates/activation.json"),
        br#"{"private":"fixture"}"#,
    )?;
    let build = signing::installed_manifest_with_mode(
        &stage,
        "linux-x64",
        &"a".repeat(40),
        42,
        "0.36.0",
        json!({"sdk":{},"browsers":{}}),
        |path| Ok(path == executable && mode & 0o111 != 0),
    )?;
    assert_eq!(
        build
            .as_object()
            .expect("build fields")
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "schema",
            "owner",
            "product",
            "version",
            "source_sha",
            "generation",
            "platform",
            "minimum_protocol",
            "components",
            "files"
        ])
    );
    assert_eq!(build["owner"], "KiaroSama");
    assert_eq!(build["generation"], 42);
    let rows = build["files"]
        .as_array()
        .expect("inventory")
        .iter()
        .map(|r| (r["path"].as_str().expect("path"), r))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        rows.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from(["numera", "LICENSE"])
    );
    assert_eq!(rows["numera"]["sha256"], ci::digest(&executable)?);
    assert_eq!(rows["numera"]["executable"], mode & 0o111 != 0);
    fs::create_dir(stage.join("data"))?;
    fs::write(stage.join("data/profile.json"), b"{}")?;
    assert!(
        signing::installed_manifest(
            &stage,
            "linux-x64",
            &"a".repeat(40),
            42,
            "0.36.0",
            json!({})
        )
        .expect_err("data must refuse publication")
        .to_string()
        .contains("publication")
    );
    Ok(())
}
pub(super) fn archive_build() -> TestResult {
    use ed25519_dalek::SigningKey;
    let tmp = Temp::new("release-archive-build")?;
    let build = json!({"generation":42,"source_sha":"a".repeat(40),"platform":"windows-x64","components":{"sdk":{},"browsers":{}}});
    let manifest = tmp.write("BUILD.json", &serde_json::to_vec(&build)?)?;
    let signature = tmp.0.join("BUILD.sig");
    let key = SigningKey::from_bytes(&[9; 32]);
    signing::sign_manifest(&manifest, &signature, &key, &key.verifying_key())?;
    let archive = tmp.0.join("numera-windows.zip");
    let mut packed = zip::ZipWriter::new(fs::File::create(&archive)?);
    for (path, name) in [
        (&manifest, "Numera Browser/BUILD.json"),
        (&signature, "Numera Browser/BUILD.sig"),
    ] {
        packed.start_file(name, zip::write::SimpleFileOptions::default())?;
        packed.write_all(&fs::read(path)?)?;
    }
    packed.finish()?;
    let mut metadata = json!({"build_manifest_sha256":ci::digest(&manifest)?,"components":build["components"],"max_members":2,"expanded_size":fs::metadata(&manifest)?.len()+fs::metadata(&signature)?.len()});
    signing::validate_archive_build(
        &archive,
        &metadata,
        "windows-x64",
        42,
        &"a".repeat(40),
        &key.verifying_key(),
    )?;
    assert!(
        signing::validate_archive_build(
            &archive,
            &metadata,
            "windows-x64",
            43,
            &"a".repeat(40),
            &key.verifying_key()
        )
        .expect_err("generation")
        .to_string()
        .contains("generation")
    );
    metadata["build_manifest_sha256"] = "0".repeat(64).into();
    assert!(
        signing::validate_archive_build(
            &archive,
            &metadata,
            "windows-x64",
            42,
            &"a".repeat(40),
            &key.verifying_key()
        )
        .expect_err("hash")
        .to_string()
        .contains("hash/signature")
    );
    Ok(())
}
pub(super) fn bundle(kind: &str) -> TestResult {
    let tmp = Temp::new("release-bundle")?;
    let app = tmp.0.join("Numera/engine");
    fs::create_dir_all(&app)?;
    let root = tmp.0.join("Numera/browsers");
    let mut env = BTreeMap::new();
    if kind != "missing" {
        fs::create_dir_all(&root)?;
    }
    if ["present", "explicit"].contains(&kind) {
        fs::create_dir(root.join("chromium-1234"))?;
        fs::write(
            root.join("chromium-1234/chrome"),
            b"owned browser fixture, never executed",
        )?;
        fs::write(
            root.join("manifest.json"),
            br#"{"playwright":{"executable":"chromium-1234/chrome"}}"#,
        )?;
    }
    if kind == "explicit" {
        env.insert(
            "PLAYWRIGHT_BROWSERS_PATH".into(),
            "/somewhere/chosen".into(),
        );
    }
    let before = env.clone();
    let found = numera_core::browser::bundled::discover(&app)?;
    let result = release::bundled_browser(&app, &tmp.0.join("data"), &env)?;
    assert_eq!(env, before, "resource discovery changed caller environment");
    if kind == "present" {
        assert_eq!(found, Some(root));
        assert!(result.options.contains_key("executable_path"));
        assert!(
            result.options["executable_path"]
                .as_str()
                .expect("browser path")
                .contains("chromium-1234")
        );
    } else {
        assert!(result.options.is_empty());
        if kind == "explicit" {
            assert_eq!(env["PLAYWRIGHT_BROWSERS_PATH"], "/somewhere/chosen");
        } else {
            assert!(found.is_none());
            assert!(!env.contains_key("PLAYWRIGHT_BROWSERS_PATH"));
        }
    }
    Ok(())
}
pub(super) fn smoke(timeout: bool) -> TestResult {
    use std::{
        ffi::OsString,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::Duration,
    };
    const MARKER: &str = "NUMERA_RELEASE_SMOKE_TEST";
    let name = std::thread::current().name().expect("test name").to_owned();
    if std::env::var(MARKER).as_deref() == Ok(name.as_str()) {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input)?;
        let mut peer = TcpStream::connect(std::env::var("NUMERA_RELEASE_SMOKE_SOCKET")?)?;
        peer.write_all(&input)?;
        peer.shutdown(std::net::Shutdown::Write)?;
        if timeout {
            std::thread::sleep(Duration::from_secs(30));
        } else {
            std::io::stdout().write_all(br#"{"ok":true}"#)?;
        }
        return Ok(());
    }
    let tmp = Temp::new("release-smoke")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let mut env = std::env::vars().collect::<BTreeMap<_, _>>();
    env.insert(MARKER.into(), name.clone());
    env.insert(
        "NUMERA_RELEASE_SMOKE_SOCKET".into(),
        listener.local_addr()?.to_string(),
    );
    let argv = ["--exact", &name, "--nocapture", "--test-threads=1"].map(OsString::from);
    let input = b"public fixture \xce\xb1\n";
    let mut received = Vec::new();
    let output = signing::smoke_with(
        &std::env::current_exe()?,
        &argv,
        input,
        &env,
        if timeout { 1 } else { 5 },
        |request| {
            assert_eq!(request.input, input);
            let mut result = signing::execute(request);
            if let Ok(output) = &mut result {
                let observed = output
                    .stdout
                    .windows(b"{\"ok\":true}".len())
                    .find(|part| *part == b"{\"ok\":true}")
                    .ok_or_else(|| {
                        io::Error::other("Native fixture did not emit its JSON result")
                    })?;
                output.stdout = observed.to_vec();
            }
            let end = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut peer, _)) => {
                        peer.set_read_timeout(Some(Duration::from_secs(1)))?;
                        peer.read_to_end(&mut received)?;
                        break;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < end =>
                    {
                        std::thread::sleep(Duration::from_millis(3))
                    }
                    Err(e) => return Err(e),
                }
            }
            result
        },
    );
    if timeout {
        assert!(
            output
                .expect_err("native timeout")
                .to_string()
                .contains("deadline")
        );
    } else {
        assert_eq!(output?, json!({"ok":true}));
    }
    assert_eq!(received, input);
    assert!(!tmp.0.join("fixture-proxies.txt").exists());
    Ok(())
}

pub(super) fn windows_signing_fixture() -> TestResult {
    pe_digest_control_boundaries()?;
    #[cfg(windows)]
    {
        windows_signing_native_fixture()
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

fn pe_digest_control_boundaries() -> TestResult {
    use numera_core::native_windows_sign::digest_control_offset;
    fn put16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let temp = Temp::new("signed-pe-control-boundaries")?;
    let path = temp.0.join("structural-only-fixture.exe");
    for (magic, optional_size, directories) in [(0x20b, 240, 112), (0x10b, 224, 96)] {
        // Hand-laid-out PE headers, not a cryptographically signed executable.
        // Only the Windows branch below establishes real signature behavior.
        let mut bytes = vec![0u8; 0x900];
        bytes[..2].copy_from_slice(b"MZ");
        put32(&mut bytes, 0x3c, 0x100);
        bytes[0x100..0x104].copy_from_slice(b"PE\0\0");
        put16(&mut bytes, 0x106, 2);
        put16(&mut bytes, 0x114, optional_size);
        put16(&mut bytes, 0x116, 2);
        let optional = 0x118;
        put16(&mut bytes, optional, magic);
        put32(&mut bytes, optional + 60, 0x400);
        put32(&mut bytes, optional + directories - 4, 16);
        put32(&mut bytes, optional + directories + 32, 0x800);
        put32(&mut bytes, optional + directories + 36, 0x100);
        let table = optional + usize::from(optional_size);
        for (index, raw, flags) in [(0, 0x400, 0x40000040), (1, 0x600, 0x60000020)] {
            let section = table + index * 40;
            put32(&mut bytes, section + 16, 0x200);
            put32(&mut bytes, section + 20, raw);
            put32(&mut bytes, section + 36, flags);
        }
        fs::write(&path, &bytes)?;
        assert_eq!(
            digest_control_offset(&path)?,
            0x600,
            "select actual executable bytes, not DOS/header data"
        );
        assert_eq!(fs::read(&path)?, bytes, "selector must be read-only");
        let executable = table + 40;
        let mut rejected = Vec::new();
        for (offset, value) in [
            (0x3c, u32::MAX),
            (optional + 60, 0x200),
            (optional + directories - 4, 4),
            (optional + directories - 4, u32::MAX),
            (optional + directories + 32, 0),
            (optional + directories + 32, 0x200),
            (optional + directories + 36, 0),
            (optional + directories + 36, u32::MAX),
            (table + 16, 0x300),
            (executable + 16, 0),
            (executable + 20, 0x200),
            (executable + 20, 0x780),
            (executable + 20, 0x880),
            (executable + 20, u32::MAX - 15),
            (executable + 36, 0x40000020),
            (executable + 36, 0x60000040),
        ] {
            let mut changed = bytes.clone();
            put32(&mut changed, offset, value);
            rejected.push(changed);
        }
        for (offset, value) in [
            (0x106, 0),
            (0x106, 97),
            (0x114, 2),
            (0x116, 0),
            (optional, 0),
        ] {
            let mut changed = bytes.clone();
            put16(&mut changed, offset, value);
            rejected.push(changed);
        }
        rejected.push(bytes[..0x100].to_vec());
        let mut bad_signature = bytes.clone();
        bad_signature[0x100] ^= 1;
        rejected.push(bad_signature);
        let mut bad_dos = bytes.clone();
        bad_dos[0] ^= 1;
        rejected.push(bad_dos);
        let mut unaligned_certificate = bytes.clone();
        put32(
            &mut unaligned_certificate,
            optional + directories + 32,
            0x809,
        );
        put32(
            &mut unaligned_certificate,
            optional + directories + 36,
            0x80,
        );
        rejected.push(unaligned_certificate);
        for (index, changed) in rejected.into_iter().enumerate() {
            fs::write(&path, &changed)?;
            assert!(
                digest_control_offset(&path).is_err(),
                "malformed PE control {magic:x}/{index} admitted"
            );
            assert_eq!(fs::read(&path)?, changed, "refused selector changed bytes");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_signing_native_fixture() -> TestResult {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::native_windows_sign;
    let provenance: Value =
        serde_json::from_str(include_str!("../fixtures/windows-signing/provenance.json"))?;
    let pfx = include_bytes!("../fixtures/windows-signing/public-fixture.pfx");
    let der = include_bytes!("../fixtures/windows-signing/public-fixture.der");
    let pin = ci::sha(der);
    assert_eq!(
        ci::sha(pfx),
        provenance["files"]["public-fixture.pfx"]["sha256"]
    );
    assert_eq!(pin, provenance["files"]["public-fixture.der"]["sha256"]);
    // The public fixture cannot authenticate an owner publication.
    let owner_pem = fs::read_to_string(root().join("docs/code-signing-cert.pem"))?;
    let owner_der = STANDARD.decode(
        owner_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>(),
    )?;
    assert_ne!(pin, ci::sha(&owner_der));
    let engine = std::env::var_os("NUMERA_TEST_SIGNING_ENGINE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("engine-rust/target/release/numera-engine.exe"));
    assert!(
        engine.is_file(),
        "Windows signing coverage requires the freshly built native numera-engine.exe"
    );
    let original_hash = ci::digest(&engine)?;
    let worker = engine
        .parent()
        .ok_or("Native engine build directory missing")?
        .join("numera-browser-runtime.exe");
    assert!(
        worker.is_file(),
        "Windows signing coverage requires the freshly built native SDK worker"
    );
    let worker_hash = ci::digest(&worker)?;
    let environment = std::env::vars().collect();
    let identity = signing::execute_with(
        signing::ToolRequest {
            program: engine.to_str().ok_or("Engine path not Unicode")?.into(),
            args: vec!["--build-info".into()],
            environment,
            input: Vec::new(),
            seconds: 15,
        },
        0,
        None,
        &mut signing::execute,
    )?;
    let identity: Value = serde_json::from_slice(&identity)?;
    assert_eq!(identity["owner"], "KiaroSama");
    assert_eq!(identity["product"], "Numera Browser");
    assert_eq!(identity["application_runtime"], "native");
    assert_eq!(identity["acceptance"], false);
    assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
    if let Ok(expected) = std::env::var("GITHUB_SHA") {
        assert_eq!(identity["source_sha"], expected);
    }
    let tmp = Temp::new("public-windows-signing")?;
    native_windows_sign::indirect_digest_controls(&tmp.0)?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir_all(stage.join("engine"))?;
    let paths = [
        stage.join("numera.exe"),
        stage.join("engine/numera-engine.exe"),
        stage.join("engine/numera-browser-runtime.exe"),
    ];
    for path in &paths[..2] {
        fs::copy(&engine, path)?;
    }
    fs::copy(&worker, &paths[2])?;
    let receipt = native_windows_sign::sign(
        &stage,
        pfx,
        provenance["password"]
            .as_str()
            .ok_or("Public fixture password missing")?,
        &pin,
        env!("CARGO_PKG_VERSION"),
    )?;
    assert_eq!(receipt["certificate_sha256"], pin);
    assert_eq!(receipt["owner"], "KiaroSama");
    assert_eq!(receipt["trust"], "owner-self-signed-private-trust");
    assert_eq!(receipt["notarized"], false);
    assert_eq!(receipt["publicly_trusted"], false);
    assert_eq!(
        receipt["files"]
            .as_array()
            .ok_or("Signed native file list missing")?
            .len(),
        3
    );
    let wrong = format!(
        "{}{}",
        if pin.starts_with('0') { '1' } else { '0' },
        &pin[1..]
    );
    for (index, path) in paths.iter().enumerate() {
        native_windows_sign::verify(path, &pin)?;
        let signed_hash = ci::digest(path)?;
        assert!(native_windows_sign::verify(path, &wrong).is_err());
        native_windows_sign::verify(path, &pin)?;
        let damaged = tmp.0.join(format!("tampered-{index}.exe"));
        let mut bytes = fs::read(path)?;
        assert!(bytes.len() > 64);
        bytes[64] ^= 1;
        fs::write(&damaged, &bytes)?;
        assert!(native_windows_sign::verify(&damaged, &pin).is_err());
        assert_eq!(ci::digest(path)?, signed_hash);
    }
    assert_eq!(
        ci::digest(&engine)?,
        original_hash,
        "fixture signing modified the build artifact"
    );
    assert_eq!(
        ci::digest(&worker)?,
        worker_hash,
        "fixture signing modified the native SDK build artifact"
    );
    Ok(())
}
