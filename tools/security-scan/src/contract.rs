//! Pure scanner contracts shared with the maintained native CI cases.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    io,
};
pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const VERSION: &str = "8.30.1";
pub const BINARY_HASH: &str = "88f91962aa2f93ac6ab281d553b9e125f5197bbbce38f9f2437f7299c32e5509";
pub const OFFICIAL_HASH: &str = "e163e53b9e7e8a8511e77271e2b323ed057759542a6d988258afe3a1fa329caf";
pub const REVIEWED_HASH: &str = "e8bacdc1cbd76ae1495ef4ec5a917030761093f793dc4a89bb512671600c5ca6";
pub const FIXTURES: [(&str, &str, u64); 3] = [
    (
        "public-fixture.der",
        "8fedb7633c7d447b6607375392c9bb02b549ed7c5fbaf7b562312f50d1edd7db",
        802,
    ),
    (
        "public-fixture.pfx",
        "4bc4ea7fa927262945f0eed6418d1a60b7f43e69d773007c79e6c7d129480782",
        2563,
    ),
    (
        "provenance.json",
        "9373b346cbc88ab826f9fb59d526c96f3cd3aa720fa5a513958cdbfb632400ae",
        937,
    ),
];
pub fn fail<T>(message: &'static str) -> Result<T> {
    Err(io::Error::other(message).into())
}
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn canonical_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub const HISTORY_BYTES: usize = 4 * 1024 * 1024;
pub const HISTORY_RECORDS: usize = 10_000;
pub const HISTORY_BLOBS: usize = 512;
const FIXTURE_DIRECTORY: &str = "tools/xtask/tests/fixtures/windows-signing/";
const ABSENT_OID: &str = "0000000000000000000000000000000000000000";

/// Every historical occurrence is checked before callers deduplicate blob reads.
/// Git's raw NUL format preserves paths, including names that quoted text loses.
pub fn history_records(raw: &[u8]) -> Result<Vec<(String, String, String)>> {
    if raw.len() > HISTORY_BYTES || (!raw.is_empty() && raw.last() != Some(&0)) {
        return fail("Signing-material history is oversized or truncated");
    }
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut parts = raw[..raw.len() - 1].split(|byte| *byte == 0);
    let mut records = Vec::new();
    let mut identities = BTreeMap::new();
    while let Some(header) = parts.next() {
        let header = std::str::from_utf8(header)?;
        let fields = header.split(' ').collect::<Vec<_>>();
        if fields.len() != 5
            || !fields[0].starts_with(':')
            || !canonical_sha(fields[2])
            || !canonical_sha(fields[3])
        {
            return fail("Signing-material history header is malformed");
        }
        let old_mode = &fields[0][1..];
        let new_mode = fields[1];
        let old_absent = fields[2] == ABSENT_OID;
        let new_absent = fields[3] == ABSENT_OID;
        let valid_mode = |mode, absent| {
            if absent {
                mode == "000000"
            } else {
                matches!(mode, "100644" | "100755")
            }
        };
        if !valid_mode(old_mode, old_absent)
            || !valid_mode(new_mode, new_absent)
            || !match fields[4] {
                "A" => old_absent && !new_absent,
                "D" => !old_absent && new_absent,
                "M" => !old_absent && !new_absent,
                _ => false,
            }
        {
            return fail("Historical signing material has an unreviewed mode or transition");
        }
        let path = std::str::from_utf8(
            parts
                .next()
                .ok_or_else(|| io::Error::other("Signing-material history path is missing"))?,
        )?;
        expected_pin(path)?;
        for oid in [fields[2], fields[3]] {
            if oid != ABSENT_OID {
                if identities
                    .insert(oid.to_owned(), path.to_owned())
                    .is_some_and(|prior| prior != path)
                {
                    return fail(
                        "Historical signing-material blob aliases different reviewed paths",
                    );
                }
                if identities.len() > HISTORY_BLOBS {
                    return fail("Historical signing-material blob inventory exceeds its bound");
                }
            }
        }
        records.push((path.to_owned(), fields[2].to_owned(), fields[3].to_owned()));
        if records.len() > HISTORY_RECORDS {
            return fail("Signing-material history record inventory exceeds its bound");
        }
    }
    Ok(records)
}

pub fn expected_pin(path: &str) -> Result<(&'static str, u64)> {
    let name = path.strip_prefix(FIXTURE_DIRECTORY).ok_or_else(|| {
        io::Error::other("Historical signing material is outside its reviewed path")
    })?;
    FIXTURES
        .iter()
        .find(|(allowed, _, _)| *allowed == name)
        .map(|(_, digest, bytes)| (*digest, *bytes))
        .ok_or_else(|| {
            io::Error::other("Historical signing material is not a reviewed fixture").into()
        })
}

/// Validate exactly one successful `git cat-file --batch` response, including
/// its object identity, framing, size and the independently pinned public bytes.
pub fn history_blob(path: &str, oid: &str, packet: &[u8]) -> Result<()> {
    let (digest, length) = expected_pin(path)?;
    if !canonical_sha(oid) || oid == ABSENT_OID || packet.len() > 16_384 {
        return fail("Historical signing-material blob identity or bound differs");
    }
    let boundary = packet
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| io::Error::other("Historical signing-material blob header is missing"))?;
    let expected = format!("{oid} blob {length}");
    let payload = &packet[boundary + 1..];
    if &packet[..boundary] != expected.as_bytes()
        || payload.len() as u64 != length + 1
        || payload.last() != Some(&b'\n')
        || hash(&payload[..payload.len().saturating_sub(1)]) != digest
    {
        return fail("Historical signing material differs from its reviewed public pin");
    }
    Ok(())
}
pub fn sanitize(text: &str) -> Result<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("[[") && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    blocks.push(current);
    let paths = regex::Regex::new(r"(?m)^\s*paths\s*=")?;
    let filtered = blocks
        .into_iter()
        .filter(|b| !(b.starts_with("[[rules.allowlists]]") && paths.is_match(b)))
        .collect::<String>();
    let inherited =
        regex::Regex::new(r"(?ms)^([ \t]*)paths = \[[ \t]*\r?\n.*?^[ \t]*\][ \t]*\r?$")?;
    if inherited.find_iter(&filtered).count() != 1 {
        return fail("Official scanner exclusions changed; inspect the pinned rules");
    }
    Ok(inherited.replace(&filtered, "${1}paths = []").into_owned())
}
pub fn configuration(
    official: &[u8],
    reviewed: &[u8],
    sanitized_path: &str,
) -> Result<(String, String)> {
    if hash(official) != OFFICIAL_HASH || hash(reviewed) != REVIEWED_HASH {
        return fail("Scanner configuration pin differs");
    }
    let reviewed = std::str::from_utf8(reviewed)?;
    if reviewed.matches("useDefault = true").count() != 1 {
        return fail("Reviewed scanner inheritance differs");
    }
    let sanitized = sanitize(std::str::from_utf8(official)?)?;
    let config = reviewed.replace(
        "useDefault = true",
        &format!("path = {}", serde_json::to_string(sanitized_path)?),
    );
    Ok((sanitized, config))
}
pub fn receipt(value: &Value, nonce: &str, git_hash: &str, shim_hash: &str) -> Result<()> {
    if value["schema"] != 1
        || value["completed"] != true
        || value["exit"] != 0
        || value["nonce"] != nonce
        || value["git_sha256"] != git_hash
        || value["shim_sha256"] != shim_hash
    {
        return fail("The actual Git-history child did not confirm successful completion");
    }
    Ok(())
}
pub fn report(exit: i32, stdout: &[u8], stderr: &[u8], bytes: &[u8]) -> Result<Vec<Value>> {
    if !matches!(exit, 0 | 2) || !stdout.is_empty() || !stderr.is_empty() {
        return fail("Scanner errors prevent a clean result");
    }
    let value: Value = serde_json::from_slice(bytes)?;
    let rows = value
        .as_array()
        .ok_or_else(|| io::Error::other("Scanner report is not an array"))?;
    if (exit == 2) != (!rows.is_empty()) {
        return fail("Scanner status and report disagree");
    }
    let mut unique = BTreeSet::new();
    for row in rows {
        if !row.is_object()
            || row["RuleID"].as_str().is_none()
            || row["File"].as_str().is_none()
            || row["StartLine"].as_u64().is_none()
            || !row["Commit"].as_str().is_some_and(canonical_sha)
        {
            return fail("Scanner finding metadata is malformed");
        }
        let identity = serde_json::to_string(&(
            row["RuleID"].clone(),
            row["File"].clone(),
            row["StartLine"].clone(),
            row["Commit"].clone(),
        ))?;
        if !unique.insert(identity) {
            return fail("Scanner report duplicates a finding");
        }
    }
    Ok(rows.clone())
}
pub fn summary(rows: &[Value], scope: &str) -> String {
    fn cell(value: &Value) -> String {
        let s = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('|', "&#124;")
            .replace(['\n', '\r'], " ")
    }
    let mut out = format!(
        "## Gitleaks\n\nScanner: **{VERSION}**\n\nScope: **{scope}**\n\nExact pinned rules; reviewed public-data exceptions remain path/value-bound. Repository ignore files are disabled.\n\n"
    );
    if rows.is_empty() {
        out.push_str("No matching credentials found in the scanned Git history.\n");
    } else {
        out.push_str(&format!("**{} potential secrets detected.** Only redacted locations are shown.\n\n| Rule | File | Line | Commit |\n| --- | --- | --- | --- |\n",rows.len()));
        for row in rows.iter().take(200) {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&row["RuleID"]),
                cell(&row["File"]),
                cell(&row["StartLine"]),
                cell(&row["Commit"])
            ));
        }
        if rows.len() > 200 {
            out.push_str("\nShowing the first200 locations; repeat the pinned scan privately for the complete report.\n");
        }
    }
    out
}
