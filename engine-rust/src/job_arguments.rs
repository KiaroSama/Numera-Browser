//! Private detached arguments use a per-start key that never enters job files.
use base64::{Engine, engine::general_purpose::URL_SAFE};
use fernet::Fernet;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub(crate) const KEY_ENV: &str = "NUMERA_JOB_ARGUMENT_KEY";
pub(crate) const MAX_PLAINTEXT: usize = 1_048_576;
// The request contains both authenticated ciphertext and redacted display argv.
pub(crate) const MAX_LAUNCH: u64 = 4 * MAX_PLAINTEXT as u64;
const VERSION: u8 = 1;
const REDACTED: &str = "[REDACTED]";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    version: u8,
    id: String,
    root: PathBuf,
    argv: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    ciphertext: String,
}

pub(crate) struct Sealed {
    pub(crate) envelope: Value,
    pub(crate) public: Vec<String>,
    pub(crate) key: Zeroizing<String>,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Detached arguments are missing, invalid, or do not match this job",
    )
}

fn sensitive(option: &str) -> bool {
    // CLI long options accept unique prefixes. Over-redacting ambiguous prefixes
    // is safe because only the authenticated private argv is ever executed.
    option.len() > 2
        && [
            "--password",
            "--new-password",
            "--pass",
            "--token",
            "--proxy",
            "--rotate-url",
        ]
        .iter()
        .any(|known| known.starts_with(option))
}

fn public_argv(argv: &[String]) -> Vec<String> {
    let mut secret_value = false;
    argv.iter()
        .map(|argument| {
            let (option, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), false), |(option, _)| (option, true));
            if std::mem::take(&mut secret_value) {
                // An invalid missing value must not hide the next sensitive
                // option from redaction; private validation still rejects it.
                secret_value = sensitive(option) && !inline;
                return REDACTED.into();
            }
            if sensitive(option) {
                if inline {
                    format!("{option}={REDACTED}")
                } else {
                    secret_value = true;
                    argument.clone()
                }
            } else {
                crate::log_redaction::redact(argument)
            }
        })
        .collect()
}

pub(crate) fn seal(id: &str, root: &Path, argv: &[String]) -> io::Result<Sealed> {
    let plaintext = Zeroizing::new(
        serde_json::to_vec(&Arguments {
            version: VERSION,
            id: id.into(),
            root: root.into(),
            argv: argv.into(),
        })
        .map_err(|_| invalid())?,
    );
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Job arguments exceed their size limit",
        ));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(key.as_mut_slice())
        .map_err(|_| io::Error::other("Detached argument key generation failed"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| io::Error::other("Detached argument timestamp is invalid"))?
        .as_secs();
    let ciphertext = crate::vault::seal_ephemeral(&key, &plaintext, timestamp)
        .map_err(|_| io::Error::other("Detached argument encryption failed"))?;
    Ok(Sealed {
        envelope: json!({"version":VERSION,"ciphertext":ciphertext}),
        public: public_argv(argv),
        key: Zeroizing::new(URL_SAFE.encode(key.as_slice())),
    })
}

pub(crate) fn open(
    request: &Value,
    id: &str,
    root: &Path,
    key: Option<&str>,
) -> io::Result<Vec<String>> {
    let envelope: Envelope =
        serde_json::from_value(request["privateArgv"].clone()).map_err(|_| invalid())?;
    if envelope.version != VERSION || envelope.ciphertext.len() as u64 > MAX_LAUNCH {
        return Err(invalid());
    }
    let cipher = key.and_then(Fernet::new).ok_or_else(invalid)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(&envelope.ciphertext)
            .map_err(|_| invalid())?,
    );
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(invalid());
    }
    let arguments: Arguments = serde_json::from_slice(&plaintext).map_err(|_| invalid())?;
    if arguments.version != VERSION
        || arguments.id != id
        || arguments.root != root
        || json!(public_argv(&arguments.argv)) != request["argv"]
    {
        return Err(invalid());
    }
    Ok(arguments.argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(sealed: &Sealed) -> Value {
        json!({"argv":sealed.public,"privateArgv":sealed.envelope})
    }

    #[test]
    fn authenticated_arguments_round_trip_without_public_credentials()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = Path::new("fixture-root");
        let argv = [
            "pool",
            "edit",
            "sample",
            "--pass",
            "synthetic-proxy-password",
            "--new-password=synthetic-new-master",
            "--tok=synthetic-token",
            "--proxy",
            "http://synthetic-user:synthetic-credential@localhost:8080",
            "--rotate-url=https://example.invalid/rotate/synthetic-path-token",
            "unlabelled://synthetic-user:synthetic-unlabelled-password@localhost",
            "--pass",
            "--token",
            "synthetic-adjacent-secret",
        ]
        .map(String::from);
        let sealed = seal("job-a", root, &argv)?;
        let request = request(&sealed);
        let encoded = serde_json::to_string(&request)?;
        for credential in [
            "synthetic-proxy-password",
            "synthetic-new-master",
            "synthetic-token",
            "synthetic-credential",
            "synthetic-path-token",
            "synthetic-unlabelled-password",
            "synthetic-adjacent-secret",
        ] {
            assert!(
                !encoded.contains(credential),
                "Public job metadata contains a credential"
            );
        }
        assert!(!encoded.contains(sealed.key.as_str()));
        assert!(
            open(&request, "job-a", root, Some(&sealed.key))? == argv,
            "Private argv changed during authenticated handoff"
        );
        Ok(())
    }

    #[test]
    fn handoff_rejects_missing_keys_and_changed_identity_or_display()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = Path::new("fixture-root");
        let sealed = seal("job-a", root, &["list".into()])?;
        let request = request(&sealed);
        let other = seal("job-a", root, &["list".into()])?;
        for key in [None, Some("invalid-key"), Some(other.key.as_str())] {
            assert!(open(&request, "job-a", root, key).is_err());
        }
        assert!(open(&request, "job-b", root, Some(&sealed.key)).is_err());
        assert!(
            open(
                &request,
                "job-a",
                Path::new("other-root"),
                Some(&sealed.key)
            )
            .is_err()
        );
        let mut changed = request.clone();
        changed["argv"] = json!(["delete", "sample"]);
        assert!(open(&changed, "job-a", root, Some(&sealed.key)).is_err());
        changed = request.clone();
        changed["privateArgv"]["version"] = json!(2);
        assert!(open(&changed, "job-a", root, Some(&sealed.key)).is_err());
        changed = request.clone();
        changed["privateArgv"]["unrecognized"] = json!(true);
        assert!(open(&changed, "job-a", root, Some(&sealed.key)).is_err());
        changed = request;
        changed["privateArgv"]["ciphertext"] = json!("tampered-private-arguments");
        assert!(open(&changed, "job-a", root, Some(&sealed.key)).is_err());
        Ok(())
    }

    #[test]
    fn private_schema_and_plaintext_limit_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root = Path::new("fixture-root");
        let key = Zeroizing::new([3u8; 32]);
        let encoded = Zeroizing::new(URL_SAFE.encode(key.as_slice()));
        for plaintext in [
            json!({"version":2,"id":"job-a","root":root,"argv":["list"]}),
            json!({"version":1,"id":"job-a","root":root,"argv":["list"],"extra":true}),
            json!({"version":1,"id":"job-a","root":root,"argv":[12]}),
        ] {
            let ciphertext =
                crate::vault::seal_ephemeral(&key, &serde_json::to_vec(&plaintext)?, 0)?;
            let request =
                json!({"argv":["list"],"privateArgv":{"version":1,"ciphertext":ciphertext}});
            assert!(open(&request, "job-a", root, Some(&encoded)).is_err());
        }
        assert!(seal("job-a", root, &["x".repeat(MAX_PLAINTEXT)]).is_err());
        Ok(())
    }
}
