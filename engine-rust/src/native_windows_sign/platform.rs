//! Windows FFI boundary. All handles below are owned RAII values; pointers refer
//! to bounded, live buffers for the duration of the synchronous Windows call.
#![allow(unsafe_code)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::GetLastError,
        Security::{Cryptography::*, WinTrust::*},
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
    },
    core::GUID,
};

fn refusal(message: &'static str) -> io::Error {
    io::Error::other(message)
}
fn last(message: &'static str) -> io::Error {
    // SAFETY: GetLastError has no arguments and only reads this thread's state.
    io::Error::other(format!("{message} (Windows error {})", unsafe {
        GetLastError()
    }))
}
fn wide(value: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let mut text: Vec<u16> = value.encode_wide().collect();
    if text.contains(&0) {
        return Err(refusal("Native signing path contains NUL"));
    }
    text.push(0);
    Ok(text)
}
struct Password(Vec<u16>);
impl Drop for Password {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}
struct Store(HCERTSTORE);
impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: this guard owns the store returned by PFXImportCertStore/CryptQueryObject.
        unsafe {
            CertCloseStore(self.0, 0);
        }
    }
}
struct Certificate(*const CERT_CONTEXT);
impl Drop for Certificate {
    fn drop(&mut self) {
        // SAFETY: this guard owns a duplicated/found certificate context.
        unsafe {
            CertFreeCertificateContext(self.0);
        }
    }
}
struct Message(*mut c_void);
impl Drop for Message {
    fn drop(&mut self) {
        // SAFETY: this guard owns the message returned by CryptQueryObject.
        unsafe {
            CryptMsgClose(self.0);
        }
    }
}
fn digest(certificate: *const CERT_CONTEXT) -> io::Result<String> {
    if certificate.is_null() {
        return Err(refusal("Native executable has no signer certificate"));
    }
    // SAFETY: Windows returned a live context retained by the enclosing guard/store.
    let context = unsafe { &*certificate };
    if context.pbCertEncoded.is_null()
        || context.cbCertEncoded == 0
        || context.cbCertEncoded > 1024 * 1024
    {
        return Err(refusal("Signer certificate exceeds its bounded encoding"));
    }
    // SAFETY: the context owns cbCertEncoded bytes and remains alive throughout hashing.
    let der = unsafe {
        std::slice::from_raw_parts(context.pbCertEncoded, context.cbCertEncoded as usize)
    };
    Ok(Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn find_certificate(store: &Store, fingerprint: &str) -> io::Result<Certificate> {
    let mut previous = null();
    loop {
        // SAFETY: store is live; enumeration releases the previous enumeration context.
        let certificate = unsafe { CertEnumCertificatesInStore(store.0, previous) };
        if certificate.is_null() {
            return Err(refusal(
                "Protected certificate differs from approved code signer",
            ));
        }
        previous = certificate;
        let matched = digest(certificate).is_ok_and(|actual| actual == fingerprint);
        if matched {
            // SAFETY: duplicate becomes independently owned, then release enumeration's context.
            let owned = unsafe { CertDuplicateCertificateContext(certificate) };
            let duplicate_error = owned
                .is_null()
                .then(|| last("Could not retain approved code signer"));
            unsafe {
                CertFreeCertificateContext(certificate);
            }
            if let Some(error) = duplicate_error {
                return Err(error);
            }
            return Ok(Certificate(owned));
        }
    }
}
fn regular(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(refusal("Owned signing target is not a regular file"));
    }
    Ok(())
}
fn identity(path: &Path, expected_version: &str) -> io::Result<()> {
    regular(path)?;
    let name = wide(path.as_os_str())?;
    let mut unused = 0;
    // SAFETY: name is terminated UTF-16; output points to an initialized scalar.
    let size = unsafe { GetFileVersionInfoSizeW(name.as_ptr(), &mut unused) };
    if size == 0 || size > 1024 * 1024 {
        return Err(refusal("Native executable product version is missing"));
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: data has the size reported above and both buffers remain live.
    if unsafe { GetFileVersionInfoW(name.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return Err(last("Native executable product version cannot be read"));
    }
    let translation: Vec<u16> = "\\VarFileInfo\\Translation"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut translated = null_mut();
    let mut translated_size = 0;
    // SAFETY: the query borrows live version bytes and returns a pointer into them.
    if unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            translation.as_ptr(),
            &mut translated,
            &mut translated_size,
        )
    } == 0
        || translated.is_null()
        || translated_size < 4
        || (translated as usize) < data.as_ptr() as usize
        || (translated as usize)
            .checked_add(translated_size as usize)
            .is_none_or(|end| end > data.as_ptr() as usize + data.len())
    {
        return Err(refusal(
            "Native executable has no version-resource translation",
        ));
    }
    // SAFETY: the checked four bytes hold two WORD values; unaligned reads support any resource offset.
    let language = unsafe { translated.cast::<u16>().read_unaligned() };
    let codepage = unsafe { translated.cast::<u16>().add(1).read_unaligned() };
    let query = |key: &str| -> io::Result<String> {
        let key: Vec<u16> = key.encode_utf16().chain([0]).collect();
        let mut pointer = null_mut();
        let mut length = 0;
        // SAFETY: the API returns a borrowed pointer into data for this terminated query.
        if unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                key.as_ptr(),
                &mut pointer,
                &mut length,
            )
        } == 0
            || pointer.is_null()
        {
            return Err(refusal("Native executable product identity is incomplete"));
        }
        let start = pointer as usize;
        let base = data.as_ptr() as usize;
        let bytes = (length as usize)
            .checked_mul(2)
            .ok_or_else(|| refusal("Native identity length overflow"))?;
        if start < base
            || start
                .checked_add(bytes)
                .is_none_or(|end| end > base + data.len())
            || !start.is_multiple_of(2)
        {
            return Err(refusal("Native executable product identity is malformed"));
        }
        // SAFETY: bounds and UTF-16 alignment were checked and data remains live.
        let value = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), length as usize) };
        String::from_utf16(value.strip_suffix(&[0]).unwrap_or(value))
            .map_err(|_| refusal("Native identity is not valid UTF-16"))
    };
    for (field, expected) in [
        ("CompanyName", "KiaroSama"),
        ("ProductName", "Numera Browser"),
        ("FileVersion", expected_version),
        ("ProductVersion", expected_version),
    ] {
        if query(&format!(
            "\\StringFileInfo\\{language:04x}{codepage:04x}\\{field}"
        ))? != expected
        {
            return Err(refusal(
                "Native executable embedded product identity differs from approved build",
            ));
        }
    }
    Ok(())
}
#[derive(Debug)]
struct TrustFailure(i32);
impl std::fmt::Display for TrustFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Authenticode file digest verification failed (0x{:08x})",
            self.0 as u32
        )
    }
}
impl std::error::Error for TrustFailure {}
fn verify_file_digest(path: &Path) -> io::Result<()> {
    let name = wide(path.as_os_str())?;
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: name.as_ptr(),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_HASH_ONLY_FLAG | WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        ..Default::default()
    };
    let mut action = GUID::from_u128(0x00aac56b_cd44_11d0_8cc2_00c04fc295ee);
    // SAFETY: the policy GUID, data and file structures retain all pointees until both synchronous calls finish.
    let code = unsafe {
        WinVerifyTrust(
            (-1isize) as _,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: release only the policy state allocated by the preceding verification.
    unsafe {
        WinVerifyTrust(
            (-1isize) as _,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    if code != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            TrustFailure(code),
        ));
    }
    Ok(())
}
fn signed_blob(path: &Path) -> io::Result<Vec<u8>> {
    fn u16_at(file: &mut fs::File, offset: u64) -> io::Result<u16> {
        let mut b = [0; 2];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut b)?;
        Ok(u16::from_le_bytes(b))
    }
    fn u32_at(file: &mut fs::File, offset: u64) -> io::Result<u32> {
        let mut b = [0; 4];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    let pe = u64::from(u32_at(&mut file, 0x3c)?);
    if u32_at(&mut file, pe)? != 0x00004550 {
        return Err(refusal("Invalid native executable header"));
    }
    let entry = pe
        + 24
        + match u16_at(&mut file, pe + 24)? {
            0x20b => 112,
            0x10b => 96,
            _ => return Err(refusal("Unsupported PE optional header")),
        }
        + 32;
    let offset = u64::from(u32_at(&mut file, entry)?);
    let size = u64::from(u32_at(&mut file, entry + 4)?);
    if offset == 0
        || !(8..=1024 * 1024).contains(&size)
        || offset.checked_add(size).is_none_or(|end| end > length)
    {
        return Err(refusal("Invalid Authenticode certificate table"));
    }
    let actual = u64::from(u32_at(&mut file, offset)?);
    if actual < 8
        || actual > size
        || u16_at(&mut file, offset + 4)? != 0x200
        || u16_at(&mut file, offset + 6)? != 2
    {
        return Err(refusal("Unsupported Authenticode certificate record"));
    }
    let mut bytes = vec![0; actual as usize - 8];
    file.seek(SeekFrom::Start(offset + 8))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn message_parameter(message: &Message, kind: u32) -> io::Result<Vec<usize>> {
    let mut size = 0;
    // SAFETY: the first call asks only for the bounded output size of a live CMS message.
    if unsafe { CryptMsgGetParam(message.0, kind, 0, null_mut(), &mut size) } == 0
        || size == 0
        || size > 1024 * 1024
    {
        return Err(last("Authenticode signer information is invalid"));
    }
    let mut words = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    // SAFETY: word storage is aligned and at least size bytes long; pointers remain valid until the caller finishes.
    if unsafe { CryptMsgGetParam(message.0, kind, 0, words.as_mut_ptr().cast(), &mut size) } == 0 {
        return Err(last("Authenticode signer information cannot be read"));
    }
    Ok(words)
}
pub(super) fn verify(path: &Path, expected: &str) -> io::Result<()> {
    regular(path)?;
    verify_file_digest(path)?;
    let mut bytes = signed_blob(path)?;
    let blob = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut store = null_mut();
    let mut message = null_mut();
    // SAFETY: blob owns bounded certificate bytes, output handles are immediately put in RAII guards.
    let ok = unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_BLOB,
            (&blob as *const CRYPT_INTEGER_BLOB).cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut store,
            &mut message,
            null_mut(),
        )
    };
    let query_error = (ok == 0).then(|| last("Authenticode CMS cannot be decoded"));
    // A failed query may still return partial handles. Own each immediately so
    // every failure path closes it without altering the captured Windows error.
    let store = (!store.is_null()).then(|| Store(store));
    let message = (!message.is_null()).then(|| Message(message));
    if let Some(error) = query_error {
        return Err(error);
    }
    let store = store.ok_or_else(|| refusal("Authenticode certificate store is missing"))?;
    let message = message.ok_or_else(|| refusal("Authenticode CMS message is missing"))?;
    let count = message_parameter(&message, CMSG_SIGNER_COUNT_PARAM)?;
    if count[0] as u32 != 1 {
        return Err(refusal("Unexpected Authenticode signer count"));
    }
    let info = message_parameter(&message, CMSG_SIGNER_CERT_INFO_PARAM)?;
    // SAFETY: info is aligned CMS-provided CERT_INFO storage and both CMS/store handles are live.
    let certificate = unsafe {
        CertFindCertificateInStore(
            store.0,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_SUBJECT_CERT,
            info.as_ptr().cast(),
            null(),
        )
    };
    if certificate.is_null() {
        return Err(last("Authenticode signer certificate is missing"));
    }
    let certificate = Certificate(certificate);
    if digest(certificate.0)? != expected {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Native executable signer differs from approved certificate",
        ));
    }
    let parameters = CMSG_CTRL_VERIFY_SIGNATURE_EX_PARA {
        cbSize: size_of::<CMSG_CTRL_VERIFY_SIGNATURE_EX_PARA>() as u32,
        dwSignerType: CMSG_VERIFY_SIGNER_CERT,
        pvSigner: certificate.0.cast_mut().cast(),
        ..Default::default()
    };
    // SAFETY: the signer certificate and parameters remain live for complete cryptographic CMS verification.
    if unsafe {
        CryptMsgControl(
            message.0,
            0,
            CMSG_CTRL_VERIFY_SIGNATURE_EX,
            (&parameters as *const CMSG_CTRL_VERIFY_SIGNATURE_EX_PARA).cast(),
        )
    } == 0
    {
        return Err(last("Authenticode CMS signature verification failed"));
    }
    Ok(())
}
fn sign_file(path: &Path, certificate: &Certificate) -> io::Result<()> {
    // Embed only the selected signer. A memory-only public certificate store
    // avoids importing a root into Windows trust or leaking unrelated PFX chain
    // entries into the executable.
    // SAFETY: the memory provider requires no parameter and returns an owned store.
    let leaf = unsafe {
        CertOpenStore(
            CERT_STORE_PROV_MEMORY,
            0,
            0,
            CERT_STORE_CREATE_NEW_FLAG,
            null(),
        )
    };
    if leaf.is_null() {
        return Err(last("Could not create an ephemeral signer store"));
    }
    let leaf = Store(leaf);
    // SAFETY: both contexts remain owned; the memory store duplicates the public context.
    if unsafe {
        CertAddCertificateContextToStore(leaf.0, certificate.0, CERT_STORE_ADD_ALWAYS, null_mut())
    } == 0
    {
        return Err(last("Could not retain the public signer certificate"));
    }
    let name = wide(path.as_os_str())?;
    let mut index = 0;
    let mut file = SIGNER_FILE_INFO {
        cbSize: size_of::<SIGNER_FILE_INFO>() as u32,
        pwszFileName: name.as_ptr(),
        ..Default::default()
    };
    let subject = SIGNER_SUBJECT_INFO {
        cbSize: size_of::<SIGNER_SUBJECT_INFO>() as u32,
        pdwIndex: &mut index,
        dwSubjectChoice: SIGNER_SUBJECT_FILE,
        Anonymous: SIGNER_SUBJECT_INFO_0 {
            pSignerFileInfo: &mut file,
        },
    };
    let mut store_info = SIGNER_CERT_STORE_INFO {
        cbSize: size_of::<SIGNER_CERT_STORE_INFO>() as u32,
        pSigningCert: certificate.0,
        dwCertPolicy: SIGNER_CERT_POLICY_STORE,
        hCertStore: leaf.0,
    };
    let signer = SIGNER_CERT {
        cbSize: size_of::<SIGNER_CERT>() as u32,
        dwCertChoice: SIGNER_CERT_STORE,
        Anonymous: SIGNER_CERT_0 {
            pCertStoreInfo: &mut store_info,
        },
        ..Default::default()
    };
    let algorithm = SIGNER_SIGNATURE_INFO {
        cbSize: size_of::<SIGNER_SIGNATURE_INFO>() as u32,
        algidHash: CALG_SHA_256,
        dwAttrChoice: SIGNER_NO_ATTR,
        ..Default::default()
    };
    let mut context = null_mut();
    // SAFETY: all structures and their pointees remain live; the ephemeral PFX store supplies the private key. No timestamp/network or UI is requested.
    let code = unsafe {
        SignerSignEx3(
            0,
            &subject,
            &signer,
            &algorithm,
            null(),
            0,
            null(),
            null(),
            null(),
            null(),
            &mut context,
            null(),
            null(),
            null(),
        )
    };
    if !context.is_null() {
        // SAFETY: the signing API transferred this context to the caller, including on partial failure.
        unsafe {
            SignerFreeSignerContext(context);
        }
    }
    if code < 0 {
        return Err(io::Error::other(format!(
            "Native Authenticode signing failed (0x{:08x})",
            code as u32
        )));
    }
    Ok(())
}
pub(super) fn sign(
    stage: &Path,
    pfx: &[u8],
    password: &str,
    expected: &str,
    version: &str,
    scratch: &Path,
) -> io::Result<Value> {
    let stage = fs::canonicalize(stage)?;
    let mut names = vec!["numera.exe", "engine/numera-engine.exe"];
    if stage.join("engine/numera-browser-runtime.exe").exists() {
        names.push("engine/numera-browser-runtime.exe");
    }
    for name in &names {
        let path = stage.join(name);
        if !fs::canonicalize(&path)?.starts_with(&stage) {
            return Err(refusal("Owned signing target escapes its stage"));
        }
        use std::os::windows::fs::MetadataExt;
        if fs::symlink_metadata(
            path.parent()
                .ok_or_else(|| refusal("Signing target parent missing"))?,
        )?
        .file_attributes()
            & 0x400
            != 0
        {
            return Err(refusal("Owned signing directory is a reparse point"));
        }
        identity(&path, version)?;
    }
    let mut pass = Password(password.encode_utf16().chain([0]).collect());
    let mut encoded = pfx.to_vec();
    let blob = CRYPT_INTEGER_BLOB {
        cbData: encoded.len() as u32,
        pbData: encoded.as_mut_ptr(),
    };
    // SAFETY: the password and bounded PFX blob remain live throughout import. NO_PERSIST_KEY forbids storing private keys on disk or in a machine/user key store.
    let handle = unsafe {
        PFXImportCertStore(
            &blob,
            pass.0.as_ptr(),
            PKCS12_NO_PERSIST_KEY | PKCS12_ALWAYS_CNG_KSP | CRYPT_USER_KEYSET,
        )
    };
    encoded.fill(0);
    pass.0.fill(0);
    if handle.is_null() {
        return Err(last("Protected signing certificate could not be imported"));
    }
    let store = Store(handle);
    let certificate = find_certificate(&store, expected)?;
    let mut wrong = expected.to_owned();
    wrong.replace_range(..1, if expected.starts_with('0') { "1" } else { "0" });
    for (index, name) in names.iter().enumerate() {
        let path = stage.join(name);
        sign_file(&path, &certificate)?;
        verify(&path, expected)?;
        match verify(&path, &wrong) {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
            _ => return Err(refusal("Wrong Authenticode signer was not rejected")),
        }
        let copy = scratch.join(format!("tampered-{index}.exe"));
        fs::copy(&path, &copy)?;
        let mut output = fs::OpenOptions::new().read(true).write(true).open(&copy)?;
        output.seek(SeekFrom::Start(64))?;
        let mut byte = [0];
        output.read_exact(&mut byte)?;
        byte[0] ^= 1;
        output.seek(SeekFrom::Start(64))?;
        output.write_all(&byte)?;
        output.sync_all()?;
        drop(output);
        match verify(&copy, expected) {
            Err(error)
                if error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<TrustFailure>())
                    .is_some_and(|cause| cause.0 as u32 == 0x80096010) => {}
            _ => {
                return Err(refusal(
                    "Tampered Authenticode file was not rejected by its file digest",
                ));
            }
        }
        verify(&path, expected)?;
    }
    let record = json!({"owner":"KiaroSama","certificate_sha256":expected,"platform":"windows-x64","trust":"owner-self-signed-private-trust","notarized":false,"publicly_trusted":false,"files":names});
    Ok(record)
}
