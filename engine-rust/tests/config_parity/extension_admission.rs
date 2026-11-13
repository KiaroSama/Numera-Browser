use super::{ConfigStore, common, fs};
use numera_core::storage::config::ConfigError;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn extension_setter_preserves_original_stored_set_admission_and_cache()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(root) = std::env::var_os("NUMERA_TEST_EXTENSION_HOME") {
        return home_extension_worker(std::path::Path::new(&root));
    }
    let temporary = common::TempDirectory::new("extension-set-admission")?;
    let extension = temporary.path().join("valid-extension");
    fs::create_dir(&extension)?;
    fs::write(extension.join("manifest.json"), b"{}")?;
    let cases = super::frozen::load(
        include_str!(
            "../fixtures/characterization/extension_setter_preserves_original_stored_set_admission_and_cache.json"
        ),
        "/workspace/work/native-characterization-acb0ivq7",
        temporary.path(),
    )?;
    assert_eq!(cases.len(), 60, "Extension set admission cases missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let store = ConfigStore::new(root);
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        let warm: Option<Vec<u8>> = serde_json::from_value(case["warm"].clone())?;
        let stamp = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        if let Some(warm) = &warm {
            fs::write(&path, warm)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_modified(stamp)?;
            store.read()?;
        }
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
            if warm.is_some() {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(stamp)?;
                assert_eq!(fs::metadata(&path)?.modified()?, stamp);
            }
        }
        let wanted: Vec<String> = serde_json::from_value(case["wanted"].clone())?;
        let result = store.set_default_extensions(wanted);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "Stored extension set admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(serde_json::to_value(value)?, case["result"]),
            Err(ConfigError::Io(ref error)) if error.kind() == std::io::ErrorKind::InvalidData => {
                assert_eq!(case["error"], "UnicodeDecodeError");
            }
            Err(error) => assert_eq!(Some(error.source_kind()), case["error"].as_str()),
        }
        let expected: Option<Vec<u8>> = serde_json::from_value(case["after"].clone())?;
        // Original caller bytes retain LF; only newly written text uses the
        // source platform's text-mode newline conversion.
        #[cfg(windows)]
        let expected = expected.map(|bytes| {
            if Some(&bytes) == raw.as_ref() {
                return bytes;
            }
            String::from_utf8(bytes)
                .expect("Written JSON is UTF-8")
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
                .into_bytes()
        });
        assert_eq!(
            fs::read(&path).ok(),
            expected,
            "Extension setter bytes differ: case={index}"
        );
        if !case["error"].is_null() {
            assert_eq!(
                fs::read(&path).ok(),
                raw,
                "Refused setter replaced settings"
            );
        }
    }
    home_extension_child(temporary.path())?;
    temporary.cleanup()?;
    Ok(())
}

fn home_extension_child(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture::{self, Limits};
    use std::process::Command;

    let home = root.join("owned-home");
    fs::create_dir_all(home.join("valid-extension"))?;
    fs::write(home.join("valid-extension/manifest.json"), b"{}")?;
    let mut child = Command::new(std::env::current_exe()?);
    child.args([
        "--exact",
        "extension_admission::extension_setter_preserves_original_stored_set_admission_and_cache",
    ]);
    child.env("NUMERA_TEST_EXTENSION_HOME", &home);
    child.env("HOME", &home).env("USERPROFILE", &home);
    let output = capture::run(
        &mut child,
        Limits {
            wall: Duration::from_secs(20),
            idle: Duration::from_secs(10),
            bytes: 128 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        output.exit,
        0,
        "Home expansion child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn home_extension_worker(home: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::extensions::unloadable;

    let entries = vec!["~/valid-extension".to_owned()];
    assert!(
        unloadable(&entries).is_empty(),
        "A real home-relative extension must be loadable"
    );
    assert!(
        unloadable(&["./~/valid-extension".into()]).is_empty(),
        "Leading current-directory components must not hide home expansion"
    );
    assert_eq!(
        numera_core::storage::home_path::expand(std::path::Path::new("./plainpath"), home)?,
        std::path::PathBuf::from("plainpath")
    );
    assert_eq!(
        numera_core::storage::home_path::expand(std::path::Path::new("."), home)?,
        std::path::PathBuf::from(".")
    );
    let absolute = home.join("literal-tilde").join("~").join("x");
    assert_eq!(
        numera_core::storage::home_path::expand(&absolute, home)?,
        absolute,
        "A tilde inside an absolute path is a literal component"
    );
    let root = home.join("settings");
    fs::create_dir(&root)?;
    let store = ConfigStore::new(root.clone());
    store.set_default_extensions(entries.clone())?;
    assert_eq!(
        store.default_extensions()?,
        vec![serde_json::Value::from("~/valid-extension")]
    );
    assert_eq!(fs::read(home.join("valid-extension/manifest.json"))?, b"{}");
    assert!(unloadable(&["~/missing-extension".into()])[0].contains("not a folder"));
    fs::create_dir(home.join("without-manifest"))?;
    assert!(unloadable(&["~/without-manifest".into()])[0].contains("no manifest.json"));
    #[cfg(unix)]
    {
        let (name, actual_home) = current_account()?;
        let named = std::path::PathBuf::from(format!("~{name}"));
        assert_eq!(
            numera_core::storage::home_path::expand(&named, home)?,
            actual_home
        );
        assert_eq!(
            numera_core::storage::home_path::expand(
                std::path::Path::new(&format!("./~{name}/review-example")),
                home
            )?,
            actual_home.join("review-example")
        );
        assert_eq!(
            numera_core::storage::data_root::resolve(Some(&named), None, home, home)?,
            actual_home
        );
        assert_eq!(
            numera_core::browser::extensions::paths(&[format!("~{name}")], home, home)?,
            vec![
                actual_home
                    .canonicalize()?
                    .to_str()
                    .ok_or("Account home nontext")?
                    .to_owned()
            ]
        );
        let missing = format!("~numera-account-missing-{}", std::process::id());
        assert!(
            numera_core::storage::home_path::expand(std::path::Path::new(&missing), home).is_err()
        );
        assert_eq!(
            unloadable(&[missing.clone(), format!("./{missing}")]),
            vec![
                "Could not determine home directory.",
                "Could not determine home directory."
            ]
        );
        {
            use std::os::unix::ffi::OsStrExt;
            let suffix = std::ffi::OsStr::from_bytes(b"non-utf8-\xff");
            let owned = home.join(suffix);
            fs::create_dir(&owned)?;
            fs::write(owned.join("manifest.json"), b"{\"name\":\"owned\"}")?;
            let input = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b"./~/non-utf8-\xff"));
            let expanded = numera_core::storage::home_path::expand(&input, home)?;
            assert_eq!(expanded, owned);
            assert_eq!(
                fs::read(expanded.join("manifest.json"))?,
                b"{\"name\":\"owned\"}"
            );
        }
        assert_eq!(
            unloadable(&[missing]),
            vec!["Could not determine home directory."]
        );
    }
    Ok(())
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn current_account() -> Result<(String, std::path::PathBuf), Box<dyn std::error::Error>> {
    use std::{
        ffi::{CStr, OsStr},
        mem::MaybeUninit,
        os::unix::ffi::OsStrExt,
        ptr,
    };
    let mut length = 1024;
    loop {
        let mut buffer = vec![0u8; length];
        let mut record = MaybeUninit::<libc::passwd>::uninit();
        let mut found = ptr::null_mut();
        // SAFETY: getuid takes no pointers; the reentrant lookup writes only
        // the aligned record, retained byte buffer and initialized result slot.
        let code = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if code == libc::ERANGE && length < 1024 * 1024 {
            length *= 2;
            continue;
        }
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code).into());
        }
        assert!(
            !found.is_null(),
            "Current kernel user must have an account record"
        );
        // SAFETY: a successful getpwuid_r supplies initialized NUL-terminated
        // fields, which are copied while the retained buffer remains live.
        let (name, directory) = unsafe {
            (
                CStr::from_ptr((*found).pw_name),
                CStr::from_ptr((*found).pw_dir),
            )
        };
        return Ok((
            name.to_str()?.to_owned(),
            std::path::PathBuf::from(OsStr::from_bytes(directory.to_bytes())),
        ));
    }
}
