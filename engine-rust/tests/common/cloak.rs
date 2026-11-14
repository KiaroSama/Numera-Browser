//! Exercises native Cloak preparation inside the existing SDK boundary test.
use numera_core::browser::sdk::cloak;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Barrier},
    time::Duration,
};

fn substitute(value: &mut Value, root: &str) {
    match value {
        Value::String(text) if text.contains("$ROOT") => {
            *text = text.replace("$ROOT", root);
            if cfg!(windows) {
                *text = text.replace('/', "\\");
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| substitute(value, root)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| substitute(value, root)),
        _ => {}
    }
}
fn env(prepared: &cloak::Prepared) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    prepared.options["env"]
        .as_array()
        .ok_or("Launch environment absent")?
        .iter()
        .map(|entry| {
            Ok((
                entry["name"]
                    .as_str()
                    .ok_or("Environment key missing")?
                    .into(),
                entry["value"]
                    .as_str()
                    .ok_or("Environment value missing")?
                    .into(),
            ))
        })
        .collect()
}
fn options(root: &Path) -> Value {
    json!({"user_data_dir":root.join("profile"),"executable_path":root.join("chrome")})
}
fn seed(
    profile: &Path,
    stealth: bool,
    args: &[String],
    environment: &BTreeMap<String, String>,
) -> std::io::Result<Vec<String>> {
    let options = json!({"user_data_dir":profile,"stealth_args":stealth,"args":args});
    let prepared = cloak::prepare_with_environment(
        options
            .as_object()
            .ok_or_else(|| std::io::Error::other("Seed options missing"))?,
        environment,
    )?;
    Ok(prepared.options["args"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("Launch arguments missing"))?
        .iter()
        .filter_map(Value::as_str)
        .filter(|arg| arg.starts_with("--fingerprint="))
        .map(str::to_owned)
        .collect())
}

pub fn verify() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("native-cloak-source")?;
    let root = temporary.path();
    fs::write(root.join("chrome"), b"external binary fixture")?;
    fs::create_dir(root.join("profile"))?;
    fs::write(root.join("profile/.cloakbrowser-seed"), b"54321\n")?;
    let mut document: Value = serde_json::from_str(include_str!("../fixtures/cloak-launch.json"))?;
    let system = if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Darwin"
    } else {
        "Linux"
    };
    substitute(
        &mut document,
        root.to_str().ok_or("Fixture path must be Unicode")?,
    );
    let environment = BTreeMap::from([(
        "CLOAKBROWSER_BINARY_PATH".into(),
        root.join("chrome")
            .to_str()
            .ok_or("Binary path missing")?
            .into(),
    )]);
    let mut observed = 0;
    for row in document["cases"]
        .as_array()
        .ok_or("Cloak fixture cases missing")?
    {
        if row["system"] != system {
            continue;
        }
        let input = row["input"]
            .as_object()
            .ok_or("Cloak fixture input missing")?;
        let before = input.clone();
        let prepared = cloak::prepare_with_environment(input, &environment)?;
        assert_eq!(
            prepared.options, row["expected"],
            "Cloak upstream launch case {}",
            row["name"]
        );
        assert_eq!(input, &before, "Cloak preparation mutated options");
        assert!(prepared.denial().is_none());
        observed += 1;
    }
    assert_eq!(observed, 31, "Cloak source cases missing");
    assert_eq!(
        fs::read(root.join("profile/.cloakbrowser-seed"))?,
        b"54321\n"
    );

    let concurrent = root.join("concurrent");
    let barrier = Arc::new(Barrier::new(16));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let profile = concurrent.clone();
            let environment = environment.clone();
            std::thread::spawn(move || {
                barrier.wait();
                seed(&profile, true, &[], &environment)
            })
        })
        .collect();
    let mut seeds = Vec::new();
    for thread in threads {
        seeds.push(thread.join().map_err(|_| "Seed writer panicked")??);
    }
    assert!(
        seeds.iter().all(|seed| seed == &seeds[0]),
        "Concurrent first launches disagreed"
    );
    let stored = fs::read_to_string(concurrent.join(".cloakbrowser-seed"))?;
    assert!(stored.ends_with('\n') && !stored.contains('\r'));
    assert!((10000..=99999).contains(&stored.trim().parse::<u32>()?));
    assert_eq!(
        fs::read_dir(&concurrent)?.count(),
        1,
        "Seed temporary leaked"
    );
    for corrupt in [
        "9999\n",
        "100000\n",
        "12x34\n",
        "１２３４５\n",
        "-12345\n",
        "NaN\n",
    ] {
        fs::write(concurrent.join(".cloakbrowser-seed"), corrupt)?;
        let result = seed(&concurrent, true, &[], &environment)?;
        let content = fs::read_to_string(concurrent.join(".cloakbrowser-seed"))?;
        assert_eq!(result, vec![format!("--fingerprint={}", content.trim())]);
        assert!((10000..=99999).contains(&content.trim().parse::<u32>()?));
    }
    let untouched = root.join("explicit-seed");
    assert_eq!(
        seed(
            &untouched,
            true,
            &["--fingerprint=off".into()],
            &environment
        )?,
        vec!["--fingerprint=off"]
    );
    assert!(seed(&untouched, false, &[], &environment)?.is_empty());
    assert!(
        !untouched.exists(),
        "Explicit seed or stealth off touched profile"
    );
    fs::write(concurrent.join(".cloakbrowser-seed"), b"")?;
    let write_path = concurrent.join(".cloakbrowser-seed");
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        fs::write(write_path, b"23456\n")
    });
    assert_eq!(
        seed(&concurrent, true, &[], &environment)?,
        vec!["--fingerprint=23456"]
    );
    writer
        .join()
        .map_err(|_| "Pending seed writer panicked")??;
    let blocked = root.join("blocked-profile");
    fs::write(&blocked, b"not a directory")?;
    assert!(seed(&blocked, true, &[], &environment).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let linked = root.join("linked-profile");
        fs::create_dir(&linked)?;
        let outside = root.join("foreign-seed");
        fs::write(&outside, b"54321\n")?;
        symlink(&outside, linked.join(".cloakbrowser-seed"))?;
        assert!(seed(&linked, true, &[], &environment).is_err());
        assert_eq!(fs::read(outside)?, b"54321\n");
    }

    for malformed in [
        json!({"headless":"yes"}),
        json!({"args":[12]}),
        json!({"proxy":42}),
        json!({"locale":42}),
        json!({"env":{"A":42}}),
        json!({"geoip":true}),
        json!({"resolved_exit_ip":"invalid"}),
        json!({"backend":"patchright"}),
        json!({"unknown":true}),
        json!({"humanize":true}),
    ] {
        let mut input = options(root);
        input.as_object_mut().ok_or("Input missing")?.extend(
            malformed
                .as_object()
                .ok_or("Malformed fixture missing")?
                .clone(),
        );
        assert!(
            cloak::prepare_with_environment(
                input.as_object().ok_or("Input missing")?,
                &environment
            )
            .is_err(),
            "Malformed Cloak input admitted"
        );
    }
    let mut missing = options(root);
    missing["executable_path"] = root
        .join("missing-browser")
        .to_str()
        .ok_or("Missing browser path")?
        .into();
    assert!(
        cloak::prepare_with_environment(missing.as_object().ok_or("Input missing")?, &environment)
            .is_err()
    );

    let cache = root.join("cache");
    fs::create_dir(&cache)?;
    fs::write(cache.join("license.key"), b" cb_fixture_file \n")?;
    let mut license_env = environment.clone();
    license_env.insert(
        "CLOAKBROWSER_CACHE_DIR".into(),
        cache.to_str().ok_or("Cache path missing")?.into(),
    );
    license_env.insert("NUMERA_PASSWORD".into(), "fixture-app-secret".into());
    let mut input = options(root);
    let first =
        cloak::prepare_with_environment(input.as_object().ok_or("Input missing")?, &license_env)?;
    let child_env = env(&first)?;
    assert_eq!(child_env["CLOAKBROWSER_LICENSE_KEY"], "cb_fixture_file");
    assert!(!child_env.contains_key("NUMERA_PASSWORD"));
    let status = child_env["CLOAKBROWSER_LICENSE_STATUS_FILE"].clone();
    assert!(Path::new(&status).starts_with(root.join("profile")));
    assert!(
        !Path::new(&status).exists(),
        "Grant path created a denial file"
    );
    let foreign = Path::new(&status).with_file_name("foreign.json");
    fs::write(&foreign, b"77")?;
    for code in 76..=79 {
        assert_eq!(
            first
                .launch_error(&format!(
                    "failure <process did exit: exitCode={code}, signal=null>"
                ))
                .ok_or("License launch error not recognized")?
                .code,
            code
        );
    }
    assert!(first.launch_error("unrelated exitCode=76").is_none());
    assert!(first.launch_error("process did exit: exitCode=1").is_none());
    fs::write(&status, b"76")?;
    assert_eq!(
        first
            .denial()
            .ok_or("Post-handshake denial not recognized")?
            .code,
        76
    );
    assert!(!Path::new(&status).exists(), "Denial was not consumed");
    assert_eq!(first.denial().ok_or("Observed denial not cached")?.code, 76);
    drop(first);
    assert_eq!(
        fs::read(&foreign)?,
        b"77",
        "Cleanup removed another launch's status"
    );
    fs::remove_file(&foreign)?;
    input["license_key"] = " cb_fixture_parameter ".into();
    input["env"] = json!({"ONLY":"custom","DROP":null});
    let second =
        cloak::prepare_with_environment(input.as_object().ok_or("Input missing")?, &license_env)?;
    let child_env = env(&second)?;
    assert_eq!(
        child_env["CLOAKBROWSER_LICENSE_KEY"],
        "cb_fixture_parameter"
    );
    assert_eq!(child_env["ONLY"], "custom");
    assert!(!child_env.contains_key("CLOAKBROWSER_CACHE_DIR"));
    assert!(!child_env.contains_key("DROP"));
    let second_path = child_env["CLOAKBROWSER_LICENSE_STATUS_FILE"].clone();
    assert_ne!(second_path, status, "Launch denial paths were reused");
    fs::write(&second_path, b"{malformed")?;
    assert!(second.denial().is_none());
    assert!(!Path::new(&second_path).exists());
    fs::write(&second_path, b"79")?;
    drop(second);
    assert!(
        !Path::new(&second_path).exists(),
        "Teardown orphaned denial file"
    );

    if cfg!(target_os = "linux") {
        let cdm = root.join("WidevineCdm");
        fs::create_dir(&cdm)?;
        fs::write(cdm.join("manifest.json"), b"{}")?;
        let prepared = cloak::prepare_with_environment(
            options(root).as_object().ok_or("Input missing")?,
            &environment,
        )?;
        let hint = root.join("profile/WidevineCdm/latest-component-updated-widevine-cdm");
        let expected = serde_json::to_vec(&json!({"Path":fs::canonicalize(&cdm)?}))?;
        assert_eq!(fs::read(&hint)?, expected);
        drop(prepared);
        fs::remove_file(&hint)?;
        let mut disabled = environment.clone();
        disabled.insert("CLOAKBROWSER_WIDEVINE_CDM".into(), "".into());
        let _prepared = cloak::prepare_with_environment(
            options(root).as_object().ok_or("Input missing")?,
            &disabled,
        )?;
        assert!(
            !hint.exists(),
            "Empty explicit Widevine path fell through to auto-detection"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
