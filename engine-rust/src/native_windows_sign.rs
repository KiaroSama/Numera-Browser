//! Authenticode signing using an explicit, ephemeral certificate. These calls
//! never read ambient signing credentials or alter a Windows trust store.
use serde_json::Value;
use std::{io, path::Path};
#[cfg(any(windows, feature = "test-support"))]
mod pe;
#[cfg(windows)]
mod platform;

/// The actual signed-PE tamper selector, exposed for portable parser controls.
#[cfg(feature = "test-support")]
pub fn digest_control_offset(path: &Path) -> io::Result<u64> {
    pe::executable_byte(path)
}
#[cfg(all(windows, feature = "test-support"))]
pub fn indirect_digest_controls(root: &Path) -> io::Result<()> {
    platform::indirect_digest_controls(root)
}

fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn sign(
    stage: &Path,
    pfx: &[u8],
    password: &str,
    pinned_der_sha256: &str,
    version: &str,
) -> io::Result<Value> {
    if !fingerprint(pinned_der_sha256)
        || pfx.is_empty()
        || pfx.len() > 1024 * 1024
        || password.contains('\0')
        || version.split('.').count() != 3
        || version
            .split('.')
            .any(|v| v.is_empty() || v.parse::<u16>().is_err())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid explicit native signing inputs",
        ));
    }
    #[cfg(windows)]
    {
        let mut scratch = crate::storage::archive_paths::Staging::new(
            &std::env::temp_dir(),
            ".numera-sign-verification-",
        )?;
        let result = platform::sign(
            stage,
            pfx,
            password,
            pinned_der_sha256,
            version,
            scratch.path()?,
        );
        let cleanup = scratch.cleanup();
        match (result, cleanup) {
            (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
                "{primary}; signing verification cleanup failed: {cleanup}"
            ))),
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = stage;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Authenticode signing requires Windows",
        ))
    }
}

pub fn verify(path: &Path, pinned_der_sha256: &str) -> io::Result<()> {
    if !fingerprint(pinned_der_sha256) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid approved certificate fingerprint",
        ));
    }
    #[cfg(windows)]
    {
        platform::verify(path, pinned_der_sha256)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Authenticode verification requires Windows",
        ))
    }
}
