//! Verify and apply native signing identities without exposing signing inputs.
use super::super::{Result, fail, regular};
use super::{
    Private, ToolOutput, ToolRequest, args, execute, execute_with, owner_context, request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::{collections::BTreeMap, fs, io, path::Path};

fn macho_tamper(bytes: &[u8]) -> Result<usize> {
    let word = |offset: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or("MachO command truncated")?
                .try_into()?,
        ))
    };
    if bytes.len() < 32 || word(0)? != 0xfeedfacf || word(4)? != 0x0100000c {
        return fail("Native executable has no bounded signed content for tamper check");
    }
    let count = word(16)? as usize;
    let size = word(20)? as usize;
    if size > 1024 * 1024 || count > size / 8 || 32 + size > bytes.len() {
        return fail("MachO signature command bounds differ");
    }
    let mut cursor = 32;
    let mut offset = None;
    for _ in 0..count {
        let kind = word(cursor)?;
        let length = word(cursor + 4)? as usize;
        if length < 8 || cursor + length > 32 + size {
            return fail("MachO command bounds differ");
        }
        if kind == 0x1d {
            if offset.is_some() || length != 16 {
                return fail("MachO signature command duplicated");
            }
            let start = word(cursor + 8)? as usize;
            let len = word(cursor + 12)? as usize;
            if start <= 32 + size || len == 0 || start + len > bytes.len() {
                return fail("MachO signed content bounds differ");
            }
            offset = Some(start - 1);
        }
        cursor += length;
    }
    if cursor != 32 + size {
        return fail("MachO load command length differs");
    }
    offset.ok_or_else(|| "Native executable has no bounded signed content for tamper check".into())
}
#[derive(Clone, Debug)]
pub struct Certificate {
    pub sha256: String,
    pub sha1: String,
}
pub fn native_macos_with<F>(
    stage: &Path,
    private_root: &Path,
    certificate: &Certificate,
    transport: &[u8],
    env: &BTreeMap<String, String>,
    mut tool: F,
) -> Result<()>
where
    F: FnMut(&ToolRequest) -> io::Result<ToolOutput>,
{
    let private = Private::new(private_root)?;
    let transport_path = private.0.join("transport.p12");
    fs::write(&transport_path, transport)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&transport_path, fs::Permissions::from_mode(0o600))?;
    }
    let keychain = private.0.join("signing.keychain-db");
    let mut create = args(&["create-keychain", "-p", ""]);
    create.push(keychain.clone().into());
    execute_with(request("security", create, env), 0, None, &mut tool)?;
    let result = (|| -> Result<()> {
        let mut unlock = args(&["unlock-keychain", "-p", ""]);
        unlock.push(keychain.clone().into());
        execute_with(request("security", unlock, env), 0, None, &mut tool)?;
        let mut import = args(&["import"]);
        import.extend([
            transport_path.clone().into(),
            "-k".into(),
            keychain.clone().into(),
            "-P".into(),
            "".into(),
            "-T".into(),
            "/usr/bin/codesign".into(),
        ]);
        execute_with(request("security", import, env), 0, None, &mut tool)?;
        let mut partition = args(&[
            "set-key-partition-list",
            "-S",
            "apple-tool:,apple:,codesign:",
            "-s",
            "-k",
            "",
        ]);
        partition.push(keychain.clone().into());
        execute_with(request("security", partition, env), 0, None, &mut tool)?;
        let approved = format!("=certificate leaf = H\"{}\"", certificate.sha1);
        let wrong = format!(
            "=certificate leaf = H\"{}{}\"",
            if certificate.sha1.starts_with('0') {
                '1'
            } else {
                '0'
            },
            &certificate.sha1[1..]
        );
        let verify = |path: &Path, requirement: &str| {
            let mut argv = args(&["--verify", "--strict", "--test-requirement", requirement]);
            argv.push(path.as_os_str().into());
            request("codesign", argv, env)
        };
        let paths = [
            stage.join("numera"),
            stage.join("engine/numera-engine"),
            stage.join("engine/numera-browser-runtime"),
        ];
        for path in &paths {
            regular(path, false)?;
            let mut sign = args(&["--force", "--sign", &certificate.sha1, "--keychain"]);
            sign.extend([
                keychain.clone().into(),
                "--timestamp=none".into(),
                path.as_os_str().into(),
            ]);
            execute_with(request("codesign", sign, env), 0, None, &mut tool)?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
            execute_with(verify(path, &wrong), 3, None, &mut tool)?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
        }
        for (index, path) in paths.iter().enumerate() {
            let mut bytes = fs::read(path)?;
            let offset = macho_tamper(&bytes)?;
            bytes[offset] ^= 1;
            let tampered = private.0.join(format!("tampered-{index}"));
            fs::write(&tampered, bytes)?;
            execute_with(
                verify(&tampered, &approved),
                1,
                Some(b"code or signature modified"),
                &mut tool,
            )?;
            execute_with(verify(path, &approved), 0, None, &mut tool)?;
        }
        Ok(())
    })();
    let mut delete = args(&["delete-keychain"]);
    delete.push(keychain.into());
    let cleanup = execute_with(request("security", delete, env), 0, None, &mut tool);
    result?;
    cleanup?;
    private.finish()?;
    fs::write(
        stage.join("SIGNING.json"),
        serde_json::to_vec(
            &json!({"owner":"KiaroSama","certificate_sha256":certificate.sha256,"platform":"macos-arm64","trust":"owner-self-signed-private-trust","notarized":false,"publicly_trusted":false}),
        )?,
    )?;
    Ok(())
}
pub fn certificate(path: &Path, environment: &BTreeMap<String, String>) -> Result<Certificate> {
    regular(path, false)?;
    let invoke = |extra: &[&str]| -> Result<String> {
        let mut argv = args(&["x509", "-in"]);
        argv.push(path.as_os_str().into());
        argv.extend(args(extra));
        let bytes = execute_with(request("openssl", argv, environment), 0, None, &mut execute)?;
        Ok(String::from_utf8(bytes)?)
    };
    let subject = invoke(&["-noout", "-subject", "-nameopt", "RFC2253"])?;
    if subject.trim() != "subject=CN=KiaroSama" {
        return fail("Code signing certificate owner differs from approved identity");
    }
    let usage = invoke(&["-noout", "-ext", "extendedKeyUsage"])?;
    if !usage.contains("Code Signing") {
        return fail("Code signing certificate lacks required usage");
    }
    let mut verify = args(&["verify", "-CAfile"]);
    verify.extend([path.as_os_str().into(), path.as_os_str().into()]);
    execute_with(
        request("openssl", verify, environment),
        0,
        None,
        &mut execute,
    )?;
    let fingerprint = |algorithm: &str| -> Result<String> {
        let value = invoke(&["-noout", "-fingerprint", algorithm])?;
        let hex = value
            .trim()
            .split_once('=')
            .ok_or("Certificate fingerprint missing")?
            .1
            .replace(':', "")
            .to_lowercase();
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return fail("Certificate fingerprint malformed");
        }
        Ok(hex)
    };
    let info = Certificate {
        sha256: fingerprint("-sha256")?,
        sha1: fingerprint("-sha1")?,
    };
    if info.sha256.len() != 64 || info.sha1.len() != 40 {
        return fail("Certificate fingerprint size differs");
    }
    Ok(info)
}
pub fn native_sign(
    stage: &Path,
    platform: &str,
    public_certificate: &Path,
    private_root: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    if platform == "linux-x64" {
        return Ok(());
    }
    owner_context(environment)?;
    if !["macos-arm64", "windows-x64"].contains(&platform) {
        return fail("Native signing platform unsupported");
    }
    let expected = certificate(public_certificate, environment)?;
    let payload = environment
        .get("NUMERA_CODE_SIGNING_PFX")
        .filter(|s| !s.is_empty())
        .ok_or("Protected native signing input missing")?;
    let password = environment
        .get("NUMERA_CODE_SIGNING_PASSWORD")
        .filter(|s| !s.is_empty())
        .ok_or("Protected native signing input missing")?;
    if password.len() > 4095 || password.contains(['\n', '\r']) {
        return fail("Protected native signing password cannot be transported");
    }
    let version = (platform == "windows-x64").then_some(env!("CARGO_PKG_VERSION"));
    let mut raw = STANDARD
        .decode(payload)
        .map_err(|_| "Protected native signing input could not be decoded")?;
    if let Some(version) = version {
        let result = numera_core::native_windows_sign::sign(
            stage,
            &raw,
            password,
            &expected.sha256,
            version,
        );
        raw.fill(0);
        let receipt = result?;
        fs::write(stage.join("SIGNING.json"), serde_json::to_vec(&receipt)?)?;
        return Ok(());
    }
    let private = Private::new(private_root)?;
    let identity = private.0.join("identity.pfx");
    let pem_path = private.0.join("identity.pem");
    let transport = private.0.join("transport.p12");
    let written = fs::write(&identity, &raw);
    raw.fill(0);
    written?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&identity, fs::Permissions::from_mode(0o600))?;
    }
    let mut open = args(&["pkcs12", "-in"]);
    open.extend([
        identity.clone().into(),
        "-out".into(),
        pem_path.clone().into(),
        "-nodes".into(),
        "-passin".into(),
        "stdin".into(),
    ]);
    let mut opening = request("openssl", open, environment);
    opening.input = password.as_bytes().to_vec();
    opening.input.push(b'\n');
    execute_with(opening, 0, None, &mut execute)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&pem_path, fs::Permissions::from_mode(0o600))?;
    }
    let actual = certificate(&pem_path, environment)?;
    if actual.sha256 != expected.sha256 {
        return fail("Protected certificate differs from pinned approved code signer");
    }
    let mut export = args(&["pkcs12", "-export", "-in"]);
    export.extend([
        pem_path.into(),
        "-out".into(),
        transport.clone().into(),
        "-passout".into(),
        "pass:".into(),
    ]);
    execute_with(
        request("openssl", export, environment),
        0,
        None,
        &mut execute,
    )?;
    let mut bytes = fs::read(transport)?;
    private.finish()?;
    let result = native_macos_with(stage, private_root, &expected, &bytes, environment, execute);
    bytes.fill(0);
    result
}
