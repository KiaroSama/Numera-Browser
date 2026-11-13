use super::{ConfigStore, common, fs};
use numera_core::storage::config::ConfigError;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn extension_setter_preserves_original_stored_set_admission_and_cache()
-> Result<(), Box<dyn std::error::Error>> {
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
    temporary.cleanup()?;
    Ok(())
}
