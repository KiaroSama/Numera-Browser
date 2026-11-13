//! Historical binary exceptions are admitted by bytes, never by path alone.
use super::*;
use ci::gitleaks_contract as contract;
const ZERO: &str = "0000000000000000000000000000000000000000";
const FIRST: &str = "1111111111111111111111111111111111111111";
const SECOND: &str = "2222222222222222222222222222222222222222";
fn path(name: &str) -> String {
    format!("{FIXTURE}/{name}")
}
fn record(
    old_mode: &str,
    new_mode: &str,
    old: &str,
    new: &str,
    status: &str,
    path: &str,
) -> Vec<u8> {
    format!(":{old_mode} {new_mode} {old} {new} {status}\0{path}\0").into_bytes()
}
fn addition(oid: &str, path: &str) -> Vec<u8> {
    record("000000", "100644", ZERO, oid, "A", path)
}
fn packet(oid: &str, bytes: &[u8]) -> Vec<u8> {
    let mut result = format!("{oid} blob {}\n", bytes.len()).into_bytes();
    result.extend_from_slice(bytes);
    result.push(b'\n');
    result
}
fn pure() -> TestResult {
    let pfx = path("public-fixture.pfx");
    let der = path("public-fixture.der");
    assert!(contract::history_records(b"").is_ok());
    let mut valid = addition(FIRST, &pfx);
    valid.extend(record("100644", "100755", FIRST, SECOND, "M", &pfx));
    valid.extend(record("100755", "000000", SECOND, ZERO, "D", &pfx));
    let rows =
        contract::history_records(&valid).map_err(|error| io::Error::other(error.to_string()))?;
    assert_eq!(
        rows,
        vec![
            (pfx.clone(), ZERO.into(), FIRST.into()),
            (pfx.clone(), FIRST.into(), SECOND.into()),
            (pfx.clone(), SECOND.into(), ZERO.into())
        ]
    );
    // A repeated valid object is allowed only for the same exact approved path.
    let mut duplicate = addition(FIRST, &pfx);
    duplicate.extend(addition(FIRST, &pfx));
    assert_eq!(
        contract::history_records(&duplicate)
            .map_err(|error| io::Error::other(error.to_string()))?
            .len(),
        2
    );
    let mut alias = addition(FIRST, &pfx);
    alias.extend(addition(FIRST, &der));
    assert!(contract::history_records(&alias).is_err());
    for bad in [
        "private.pfx",
        "public-fixture.pfx",
        "nested/tools/xtask/tests/fixtures/windows-signing/public-fixture.pfx",
        "tools/xtask/tests/fixtures/windows-signing/../windows-signing/public-fixture.pfx",
        "tools/xtask/tests/fixtures/windows-signing/Public-fixture.pfx",
        "/tools/xtask/tests/fixtures/windows-signing/public-fixture.pfx",
        "tools\\xtask\\tests\\fixtures\\windows-signing\\public-fixture.pfx",
        "tools/xtask/tests/fixtures/windows-signing/unknown.p12",
        "tools/xtask/tests/fixtures/windows-signing/public-fixture.pfx\n",
    ] {
        assert!(contract::history_records(&addition(FIRST, bad)).is_err());
    }
    for mode in [
        "120000", "040000", "160000", "100664", "0100644", "", "10064a",
    ] {
        assert!(
            contract::history_records(&record("000000", mode, ZERO, FIRST, "A", &pfx)).is_err()
        );
        assert!(
            contract::history_records(&record(mode, "000000", FIRST, ZERO, "D", &pfx)).is_err()
        );
    }
    for status in ["R100", "C100", "T", "U", "A1", "", " M"] {
        assert!(
            contract::history_records(&record("100644", "100644", FIRST, SECOND, status, &pfx))
                .is_err()
        );
    }
    for invalid in [
        record("000000", "100644", FIRST, SECOND, "A", &pfx),
        record("100644", "100644", ZERO, FIRST, "M", &pfx),
        record("100644", "000000", FIRST, SECOND, "D", &pfx),
        record("000000", "000000", ZERO, ZERO, "A", &pfx),
        addition("111111111111111111111111111111111111111", &pfx),
        addition("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", &pfx),
        addition("gggggggggggggggggggggggggggggggggggggggg", &pfx),
        valid[..valid.len() - 1].to_vec(),
        format!(":000000 100644 {ZERO} {FIRST} A\0").into_bytes(),
        format!("000000 100644 {ZERO} {FIRST} A\0{pfx}\0").into_bytes(),
        format!(":000000  100644 {ZERO} {FIRST} A\0{pfx}\0").into_bytes(),
        format!("\n:000000 100644 {ZERO} {FIRST} A\0{pfx}\0").into_bytes(),
        b"\0".to_vec(),
        b"\xff\0path\0".to_vec(),
    ] {
        assert!(contract::history_records(&invalid).is_err());
    }
    assert!(contract::history_records(&vec![0; contract::HISTORY_BYTES + 1]).is_err());
    let base = addition(FIRST, &pfx);
    assert!(contract::history_records(&base.repeat(contract::HISTORY_RECORDS)).is_ok());
    assert!(contract::history_records(&base.repeat(contract::HISTORY_RECORDS + 1)).is_err());
    let mut unique = Vec::new();
    for index in 1..=contract::HISTORY_BLOBS {
        unique.extend(addition(&format!("{index:040x}"), &pfx));
    }
    assert!(contract::history_records(&unique).is_ok());
    unique.extend(addition(
        &format!("{:040x}", contract::HISTORY_BLOBS + 1),
        &pfx,
    ));
    assert!(contract::history_records(&unique).is_err());
    for (name, digest, length) in contract::FIXTURES {
        let path = path(name);
        let bytes = fs::read(root().join(&path))?;
        assert_eq!(
            contract::expected_pin(&path).map_err(|error| io::Error::other(error.to_string()))?,
            (digest, length)
        );
        let valid = packet(FIRST, &bytes);
        assert!(contract::history_blob(&path, FIRST, &valid).is_ok());
        assert!(contract::history_blob(&path, SECOND, &valid).is_err());
        assert!(contract::history_blob(&path, ZERO, &valid).is_err());
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        assert!(contract::history_blob(&path, FIRST, &packet(FIRST, &changed)).is_err());
        for bad in [
            valid[..valid.len() - 1].to_vec(),
            valid[..valid.len() - 2].to_vec(),
            [valid.as_slice(), b"\n"].concat(),
            [valid.as_slice(), valid.as_slice()].concat(),
            format!("{FIRST} missing\n").into_bytes(),
            format!("{FIRST} tree {length}\n").into_bytes(),
            format!("{FIRST} blob 0{length}\n").into_bytes(),
            b"no newline".to_vec(),
            vec![0; 16_385],
        ] {
            assert!(contract::history_blob(&path, FIRST, &bad).is_err());
        }
        assert!(contract::history_blob("unreviewed.pfx", FIRST, &valid).is_err());
    }
    Ok(())
}
fn commit(temp: &Temp, message: &str) -> TestResult {
    git(
        &temp.0,
        &[
            "-c",
            "user.name=Public contract fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )
}
fn actual() -> TestResult {
    let temp = Temp::new("historical-signing-material")?;
    git(&temp.0, &["init", "--quiet"])?;
    assert!(
        scope::history_signing_material(&temp.0).is_err(),
        "An unborn history cannot prove complete scanner scope"
    );
    let paths = install(&temp)?;
    git(&temp.0, &["add", "--", FIXTURE])?;
    commit(&temp, "Approved public fixtures")?;
    assert_eq!(scope::history_signing_material(&temp.0)?, 3);
    let head = ci::git(&temp.0, &["rev-parse", "HEAD"])?;
    fs::write(temp.0.join(".git/shallow"), &head)?;
    assert!(scope::history_signing_material(&temp.0).is_err());
    fs::remove_file(temp.0.join(".git/shallow"))?;
    let original = fs::read(&paths[1])?;
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&paths[1], &changed)?;
    git(&temp.0, &["add", "--", FIXTURE])?;
    commit(&temp, "Unreviewed historical bytes")?;
    fs::write(&paths[1], original)?;
    git(&temp.0, &["add", "--", FIXTURE])?;
    commit(&temp, "Restore public fixture bytes")?;
    assert_eq!(scope::check_utf8(&paths)?, 3);
    assert!(
        scope::history_signing_material(&temp.0).is_err(),
        "Current pinned bytes must not mask unknown historical bytes"
    );
    git(&temp.0, &["rm", "--quiet", "-r", "--", FIXTURE])?;
    commit(&temp, "Delete current fixtures")?;
    assert!(!temp.0.join(FIXTURE).exists());
    assert!(
        scope::history_signing_material(&temp.0).is_err(),
        "A deleted current fixture directory must not erase historical refusal"
    );
    let absent = Temp::new("no-historical-signing-material")?;
    git(&absent.0, &["init", "--quiet"])?;
    absent.write("README", b"Public repository without signing material\n")?;
    git(&absent.0, &["add", "README"])?;
    commit(&absent, "No signing material")?;
    assert_eq!(scope::history_signing_material(&absent.0)?, 0);
    absent.write(
        "unreviewed/elsewhere.PFX",
        &fs::read(root().join(path("public-fixture.pfx")))?,
    )?;
    git(&absent.0, &["add", "unreviewed/elsewhere.PFX"])?;
    commit(&absent, "Wrong-path public bytes")?;
    assert!(scope::history_signing_material(&absent.0).is_err());
    // A Git symlink mode is rejected regardless of host symlink privileges.
    let mode = Temp::new("historical-signing-mode")?;
    git(&mode.0, &["init", "--quiet"])?;
    install(&mode)?;
    git(&mode.0, &["add", "--", FIXTURE])?;
    commit(&mode, "Approved regular fixtures")?;
    let oid = ci::git(
        &mode.0,
        &["rev-parse", &format!("HEAD:{}", path("public-fixture.pfx"))],
    )?;
    let oid = std::str::from_utf8(&oid)?.trim();
    git(
        &mode.0,
        &[
            "update-index",
            "--cacheinfo",
            &format!("120000,{oid},{}", path("public-fixture.pfx")),
        ],
    )?;
    commit(&mode, "Unreviewed symlink mode")?;
    assert!(scope::history_signing_material(&mode.0).is_err());
    Ok(())
}
pub(super) fn check() -> TestResult {
    pure()?;
    actual()
}
