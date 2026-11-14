//! Compare authenticated CMS content with Windows' own PE image hashing.
use super::{Message, TrustFailure, last, refusal, wide};
use std::{
    io,
    mem::size_of,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Security::{
        Cryptography::{Sip::*, *},
        WinTrust::SPC_INDIRECT_DATA_OBJID,
    },
};
use windows_sys::core::GUID;

const SHA256: &[u8] = b"2.16.840.1.101.3.4.2.1";
const PE_IMAGE: &[u8] = b"1.3.6.1.4.1.311.2.1.15";
const INDIRECT_DATA: &[u8] = b"1.3.6.1.4.1.311.2.1.4\0";

struct Buffer {
    words: Vec<usize>,
    used: usize,
}
impl Buffer {
    fn new(size: u32) -> io::Result<Self> {
        let used = size as usize;
        if !(size_of::<SIP_INDIRECT_DATA>()..=1024 * 1024).contains(&used) {
            return Err(refusal("Invalid Authenticode indirect-data size"));
        }
        Ok(Self {
            words: vec![0; used.div_ceil(size_of::<usize>())],
            used,
        })
    }
    fn set_used(&mut self, size: u32) -> io::Result<()> {
        if size as usize > self.used || (size as usize) < size_of::<SIP_INDIRECT_DATA>() {
            return Err(refusal("Invalid Authenticode indirect-data result"));
        }
        self.used = size as usize;
        Ok(())
    }
    fn pointer(&mut self) -> *mut SIP_INDIRECT_DATA {
        self.words.as_mut_ptr().cast()
    }
    fn bytes(&self) -> &[u8] {
        // SAFETY: initialized aligned word storage owns at least used bytes.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr().cast(), self.used) }
    }
    fn data(&self) -> SIP_INDIRECT_DATA {
        // SAFETY: new/set_used require a full struct; usize storage supplies its
        // pointer alignment. Native output has no Rust references or drop state.
        unsafe { self.words.as_ptr().cast::<SIP_INDIRECT_DATA>().read() }
    }
    fn span(&self, pointer: *const u8, size: usize) -> io::Result<&[u8]> {
        let start = (pointer as usize)
            .checked_sub(self.words.as_ptr() as usize)
            .ok_or_else(|| refusal("Authenticode digest pointer escapes its buffer"))?;
        let end = start
            .checked_add(size)
            .ok_or_else(|| refusal("Authenticode digest range overflows"))?;
        // Read via our allocation, never by dereferencing the supplied pointer.
        self.bytes()
            .get(start..end)
            .ok_or_else(|| refusal("Authenticode digest range escapes its buffer"))
    }
    fn oid(&self, pointer: *const u8) -> io::Result<&[u8]> {
        let start = (pointer as usize)
            .checked_sub(self.words.as_ptr() as usize)
            .ok_or_else(|| refusal("Authenticode OID pointer escapes its buffer"))?;
        let remaining = self
            .bytes()
            .get(start..)
            .ok_or_else(|| refusal("Authenticode OID range escapes its buffer"))?;
        let end = remaining
            .iter()
            .take(128)
            .position(|byte| *byte == 0)
            .ok_or_else(|| refusal("Authenticode OID is not bounded"))?;
        Ok(&remaining[..end])
    }
    fn digest(&self) -> io::Result<[u8; 32]> {
        let data = self.data();
        if data.Digest.cbData != 32 {
            return Err(refusal("Authenticode image digest is not SHA256-sized"));
        }
        self.span(data.Digest.pbData, 32)?
            .try_into()
            .map_err(|_| refusal("Authenticode image digest is incomplete"))
    }
    fn authenticated_digest(&self) -> io::Result<[u8; 32]> {
        let data = self.data();
        if self.oid(data.Data.pszObjId)? != PE_IMAGE
            || self.oid(data.DigestAlgorithm.pszObjId)? != SHA256
        {
            return Err(refusal("Authenticode content must bind a SHA256 PE image"));
        }
        for blob in [data.Data.Value, data.DigestAlgorithm.Parameters] {
            if blob.cbData != 0 {
                self.span(blob.pbData, blob.cbData as usize)?;
            }
        }
        self.digest()
    }
    fn computed_digest(&self, requested: &[u8]) -> io::Result<[u8; 32]> {
        let returned = self.data().DigestAlgorithm.pszObjId;
        // SIP may reuse the caller-owned algorithm string or copy it into its
        // output allocation. Accept only those two bounded, SHA256 spellings.
        let matches = if returned.cast_const() == requested.as_ptr() {
            requested.strip_suffix(&[0]) == Some(SHA256)
        } else {
            self.oid(returned).is_ok_and(|oid| oid == SHA256)
        };
        if !matches {
            return Err(refusal(
                "Windows SIP returned an unverified digest algorithm",
            ));
        }
        self.digest()
    }
}

fn exact_sequence(bytes: &[u8]) -> bool {
    let [0x30, length, rest @ ..] = bytes else {
        return false;
    };
    if length & 0x80 == 0 {
        return usize::from(*length) == rest.len();
    }
    let count = usize::from(length & 0x7f);
    if count == 0 || count > 4 || rest.len() < count || rest[0] == 0 {
        return false;
    }
    let size = rest[..count]
        .iter()
        .fold(0usize, |size, byte| (size << 8) | usize::from(*byte));
    size >= 128 && size == rest.len() - count
}
fn decode(encoded: &[u8]) -> io::Result<Buffer> {
    if encoded.len() > 1024 * 1024 || !exact_sequence(encoded) {
        return Err(refusal("Invalid exact Authenticode content encoding"));
    }
    let mut size = 0;
    // SAFETY: input is a live bounded exact DER sequence; query allocates nothing.
    if unsafe {
        CryptDecodeObject(
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            SPC_INDIRECT_DATA_OBJID,
            encoded.as_ptr(),
            encoded.len() as u32,
            0,
            null_mut(),
            &mut size,
        )
    } == 0
    {
        return Err(last("Authenticode indirect data cannot be decoded"));
    }
    let mut decoded = Buffer::new(size)?;
    // SAFETY: owned aligned storage has the exact queried capacity. Default
    // decode flags copy OID strings and nested bytes into this same allocation.
    if unsafe {
        CryptDecodeObject(
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            SPC_INDIRECT_DATA_OBJID,
            encoded.as_ptr(),
            encoded.len() as u32,
            0,
            decoded.pointer().cast(),
            &mut size,
        )
    } == 0
    {
        return Err(last("Authenticode indirect data cannot be decoded"));
    }
    decoded.set_used(size)?;
    Ok(decoded)
}
fn message_bytes(message: &Message, kind: u32) -> io::Result<Vec<u8>> {
    let mut size = 0;
    // SAFETY: query only the size of an already authenticated live CMS message.
    if unsafe { CryptMsgGetParam(message.0, kind, 0, null_mut(), &mut size) } == 0
        || size == 0
        || size > 1024 * 1024
    {
        return Err(last("Authenticode content size is invalid"));
    }
    let mut bytes = vec![0; size as usize];
    // SAFETY: byte output has the queried size and outlives the synchronous call.
    if unsafe { CryptMsgGetParam(message.0, kind, 0, bytes.as_mut_ptr().cast(), &mut size) } == 0
        || size as usize > bytes.len()
    {
        return Err(last("Authenticode content cannot be read"));
    }
    bytes.truncate(size as usize);
    Ok(bytes)
}
struct Provider(usize);
impl Drop for Provider {
    fn drop(&mut self) {
        // SAFETY: this guard owns only a successful ephemeral provider context.
        unsafe {
            CryptReleaseContext(self.0, 0);
        }
    }
}
fn actual_digest(path: &Path) -> io::Result<[u8; 32]> {
    let name = wide(path.as_os_str())?;
    let mut subject = GUID::from_u128(0);
    // SAFETY: live filename and writable GUID; null requests file-path lookup.
    if unsafe { CryptSIPRetrieveSubjectGuid(name.as_ptr(), null_mut(), &mut subject) } == 0 {
        return Err(last("Windows PE SIP provider is unavailable"));
    }
    let mut provider = 0;
    // SAFETY: VERIFYCONTEXT creates no persistent key/container/trust state.
    if unsafe {
        CryptAcquireContextW(
            &mut provider,
            null(),
            MS_ENH_RSA_AES_PROV_W,
            PROV_RSA_AES,
            CRYPT_VERIFYCONTEXT,
        )
    } == 0
    {
        return Err(last("Windows SHA256 digest provider is unavailable"));
    }
    let provider = Provider(provider);
    let mut oid = SHA256.iter().copied().chain([0]).collect::<Vec<_>>();
    let mut info = SIP_SUBJECTINFO {
        cbSize: size_of::<SIP_SUBJECTINFO>() as u32,
        pgSubjectType: &mut subject,
        hFile: INVALID_HANDLE_VALUE,
        pwsFileName: name.as_ptr(),
        hProv: provider.0,
        DigestAlgorithm: CRYPT_ALGORITHM_IDENTIFIER {
            pszObjId: oid.as_mut_ptr(),
            ..Default::default()
        },
        dwEncodingType: X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
        ..Default::default()
    };
    let mut size = 0;
    // SAFETY: all subject/provider/algorithm pointees are live. Keep this same
    // subject between calls, as required by SIP's internal version fields.
    if unsafe { CryptSIPCreateIndirectData(&mut info, &mut size, null_mut()) } == 0 {
        return Err(last("Windows PE image digest size cannot be obtained"));
    }
    let mut actual = Buffer::new(size)?;
    // SAFETY: aligned owned output has the provider's bounded queried size.
    if unsafe { CryptSIPCreateIndirectData(&mut info, &mut size, actual.pointer()) } == 0 {
        return Err(last("Windows PE image digest cannot be computed"));
    }
    actual.set_used(size)?;
    actual.computed_digest(&oid)
}
pub(super) fn verify(path: &Path, message: &Message) -> io::Result<()> {
    if message_bytes(message, CMSG_INNER_CONTENT_TYPE_PARAM)? != INDIRECT_DATA {
        return Err(refusal(
            "Authenticated message is not Authenticode indirect data",
        ));
    }
    let expected = decode(&message_bytes(message, CMSG_CONTENT_PARAM)?)?.authenticated_digest()?;
    let actual = actual_digest(path)?;
    if actual != expected {
        // Only an explicit mismatch with the authenticated SHA256 produces this
        // category. SIP/API/parse errors never become digest-mismatch evidence.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            TrustFailure(0x80096010u32 as i32),
        ));
    }
    Ok(())
}

#[cfg(feature = "test-support")]
pub(super) fn controls(root: &Path) -> io::Result<()> {
    // Independent DER framing and literal SHA256; this is not a signed PE.
    // The real file loop below must still authenticate and hash all three PEs.
    let mut encoded = vec![
        0x30, 0x41, 0x30, 0x0c, 0x06, 0x0a, 0x2b, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01,
        0x0f, 0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
        0x01, 0x05, 0x00, 0x04, 0x20,
    ];
    encoded.extend_from_slice(&[0x5a; 32]);
    assert_eq!(decode(&encoded)?.authenticated_digest()?, [0x5a; 32]);
    let requested = SHA256.iter().copied().chain([0]).collect::<Vec<_>>();
    assert_eq!(decode(&encoded)?.computed_digest(&requested)?, [0x5a; 32]);
    let mut alias = decode(&encoded)?;
    let mut alias_data = alias.data();
    alias_data.DigestAlgorithm.pszObjId = requested.as_ptr().cast_mut();
    // SAFETY: the owned test buffer supplies a full aligned POD destination.
    unsafe {
        alias.pointer().write(alias_data);
    }
    assert_eq!(alias.computed_digest(&requested)?, [0x5a; 32]);
    assert!(alias.computed_digest(b"another\0").is_err());
    for end in 0..encoded.len() {
        assert!(
            decode(&encoded[..end])
                .and_then(|data| data.authenticated_digest())
                .is_err(),
            "truncated indirect DER admitted at {end}"
        );
    }
    let mut malformed = Vec::new();
    for (index, byte) in [(15, 0x0e), (30, 2), (33, 5), (1, 0x80)] {
        let mut value = encoded.clone();
        value[index] = byte;
        malformed.push(value);
    }
    let mut short = encoded.clone();
    short.pop();
    short[1] -= 1;
    short[17] -= 1;
    short[34] = 31;
    malformed.push(short);
    let mut trailing = encoded.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut overlong = vec![0x30, 0x81, 0x41];
    overlong.extend_from_slice(&encoded[2..]);
    malformed.push(overlong);
    for (index, value) in malformed.into_iter().enumerate() {
        assert!(
            decode(&value)
                .and_then(|data| data.authenticated_digest())
                .is_err(),
            "malformed indirect DER control {index} admitted"
        );
    }
    for size in [
        0,
        1,
        size_of::<SIP_INDIRECT_DATA>() as u32 - 1,
        1024 * 1024 + 1,
    ] {
        assert!(Buffer::new(size).is_err());
    }
    let mut bounded = Buffer::new(256)?;
    assert!(bounded.span(null(), 32).is_err());
    assert!(bounded.span(usize::MAX as *const u8, 32).is_err());
    let last = bounded.words.as_ptr() as usize + bounded.used - 1;
    assert!(bounded.span(last as *const u8, 32).is_err());
    assert!(bounded.set_used(257).is_err());
    assert!(
        bounded
            .set_used(size_of::<SIP_INDIRECT_DATA>() as u32 - 1)
            .is_err()
    );
    bounded.words.fill(usize::MAX);
    assert!(bounded.oid(bounded.words.as_ptr().cast()).is_err());
    for damage in 0..4 {
        let mut value = decode(&encoded)?;
        let mut data = value.data();
        match damage {
            0 => data.Digest.pbData = null_mut(),
            1 => data.Digest.cbData = u32::MAX,
            2 => data.Data.pszObjId = null_mut(),
            _ => data.DigestAlgorithm.pszObjId = null_mut(),
        }
        // SAFETY: our aligned full-size test buffer owns this POD structure.
        unsafe {
            value.pointer().write(data);
        }
        assert!(value.authenticated_digest().is_err());
    }
    let not_pe = root.join("indirect-not-pe.bin");
    std::fs::write(&not_pe, b"not a portable executable")?;
    for path in [&not_pe, &root.join("indirect-missing.exe")] {
        let error = actual_digest(path).expect_err("provider failure must remain failure");
        assert!(
            !error
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<TrustFailure>())
                .is_some_and(|cause| cause.0 as u32 == 0x80096010),
            "generic provider failure became BAD_DIGEST"
        );
    }
    std::fs::remove_file(not_pe)?;
    Ok(())
}
