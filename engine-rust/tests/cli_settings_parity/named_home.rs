use super::{BTreeMap, Value, cli, json};
use numera_core::storage::{config::ConfigStore, repo::ProfileRepo};
#[cfg(unix)]
use std::path::PathBuf;
use std::{fs, path::Path};

#[cfg(unix)]
#[allow(unsafe_code)]
fn current_account() -> Result<(String, PathBuf), Box<dyn std::error::Error>> {
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
            PathBuf::from(OsStr::from_bytes(directory.to_bytes())),
        ));
    }
}

pub(super) fn check(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use sha2::Digest;
    let raw = include_str!("../fixtures/cli-characterization/named-home-contract.json");
    let contract: Value = serde_json::from_str(raw)?;
    let provenance: Value = serde_json::from_str(include_str!(
        "../fixtures/cli-characterization-manifest.json"
    ))?;
    let hash = sha2::Sha256::digest(raw.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, provenance["named_home_contract"]["sha256"]);
    fs::create_dir_all(root)?;
    let repo = ProfileRepo::open(root.into(), None, None)?;
    let config = ConfigStore::new(root.into());
    config.set_many(serde_json::Map::from_iter([(
        "headless_launch".into(),
        false.into(),
    )]))?;
    let before = fs::read(root.join("config.json"))?;
    let installed = BTreeMap::new();
    let settings = cli::settings_commands::Settings {
        root,
        repo: &repo,
        installed: &installed,
        environment_token: false,
    };
    #[cfg(unix)]
    let (name, home) = current_account()?;
    #[cfg(windows)]
    let (name, home) = (
        std::env::var("USERNAME")?,
        std::env::home_dir().ok_or("Native user home missing")?,
    );
    let suffix = "numera-public-expansion-control";
    #[cfg(unix)]
    let contextual_home = root.join("unrelated-current-home");
    #[cfg(windows)]
    let contextual_home = home.clone();
    let result = cli::extension_commands::defaults(
        root,
        "default-remove",
        Some(&format!("~{name}/{suffix}")),
        &contextual_home,
    )?;
    assert_eq!(result.exit, 1);
    assert_eq!(
        result.payload["error"],
        format!("'{}' isn't in the default set", home.join(suffix).display())
    );
    assert_eq!(fs::read(root.join("config.json"))?, before);
    for (spelling, expected) in [
        (format!("./~{name}/{suffix}"), home.join(suffix)),
        (format!("./~/{suffix}"), contextual_home.join(suffix)),
        ("./plainpath".to_owned(), Path::new("plainpath").to_owned()),
        (
            home.join(suffix)
                .to_str()
                .ok_or("Account home nontext")?
                .to_owned(),
            home.join(suffix),
        ),
    ] {
        let outcome = cli::extension_commands::defaults(
            root,
            "default-remove",
            Some(&spelling),
            &contextual_home,
        )?;
        assert_eq!(outcome.exit, 1);
        assert_eq!(
            outcome.payload["error"],
            format!("'{}' isn't in the default set", expected.display())
        );
        assert_eq!(fs::read(root.join("config.json"))?, before);
    }
    let missing = format!(
        "numera-absent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .subsec_nanos()
    );
    #[cfg(unix)]
    {
        let failure = cli::extension_commands::defaults(
            root,
            "default-remove",
            Some(&format!("./~{missing}/{suffix}")),
            &contextual_home,
        )
        .err()
        .ok_or("Leading current directory must not hide an unknown account")?;
        assert_eq!(
            failure.to_string(),
            contract["unix"]["unknown_named_user_error"]
        );
        assert_eq!(fs::read(root.join("config.json"))?, before);
    }
    let request =
        json!({"headless_launch":"on","default_extension":[format!("~{missing}/{suffix}")]});
    let result = settings.config(
        request.as_object().ok_or("Settings control missing")?,
        &home,
    );
    #[cfg(unix)]
    assert_eq!(
        result
            .err()
            .ok_or("Unknown account must refuse")?
            .to_string(),
        contract["unix"]["unknown_named_user_error"]
    );
    #[cfg(windows)]
    {
        match result {
            Ok(outcome) => {
                assert_eq!(outcome.exit, 1);
                assert!(
                    outcome.payload["error"]
                        .as_str()
                        .is_some_and(|error| error.contains(&missing))
                );
            }
            Err(error) => assert_eq!(error.to_string(), "Could not determine home directory."),
        }
        for case in contract["windows"]
            .as_array()
            .ok_or("Windows expansion observations missing")?
        {
            let path = case["path"].as_str().ok_or("Source path missing")?;
            let tail = path.strip_prefix('~').ok_or("Named source path missing")?;
            let end = tail.find(['/', '\\']).unwrap_or(tail.len());
            let result = cli::settings_commands::windows_named_home(
                &tail[..end],
                Path::new(case["home"].as_str().ok_or("Source home missing")?),
                case["username"].as_str().map(std::ffi::OsStr::new),
            );
            match result {
                Ok(home) => {
                    assert!(case["error"].is_null());
                    assert_eq!(
                        home.join(tail[end..].trim_start_matches(['/', '\\']))
                            .to_str(),
                        case["expanded"].as_str()
                    );
                }
                Err(error) => assert_eq!(error.to_string(), case["error"]),
            }
        }
    }
    assert_eq!(
        fs::read(root.join("config.json"))?,
        before,
        "Refused named-user extension changed settings"
    );
    assert!(!config.headless_launch()?);
    Ok(())
}
