//! Stronger assertions inside an existing source scope case; no added cases.
use super::*;
use std::process::Command;
#[path = "secret_review/bootstrap.rs"]
mod bootstrap;
#[path = "secret_review/history.rs"]
mod signing_history;
const FIXTURE: &str = "tools/xtask/tests/fixtures/windows-signing";
fn install(temp: &Temp) -> ci::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for name in [
        "public-fixture.der",
        "public-fixture.pfx",
        "provenance.json",
    ] {
        let relative = format!("{FIXTURE}/{name}");
        paths.push(temp.write(&relative, &fs::read(root().join(&relative))?)?);
    }
    Ok(paths)
}
fn git(root: &Path, args: &[&str]) -> TestResult {
    let output = ci::capture(
        Command::new("git").arg("-C").arg(root).args(args),
        std::time::Duration::from_secs(15),
        std::time::Duration::from_secs(15),
        65536,
    )?;
    if output.exit != 0 {
        return ci::fail("Isolated scanner fixture Git operation failed");
    }
    Ok(())
}
pub(super) fn check() -> TestResult {
    bootstrap::check()?;
    signing_history::check()?;
    let temp = Temp::new("secret-scope-review")?;
    let paths = install(&temp)?;
    assert_eq!(scope::check_utf8(&paths)?, 3);
    git(&temp.0, &["init", "--quiet"])?;
    git(&temp.0, &["add", "--", FIXTURE])?;
    let staged = temp.0.join("valid-staging");
    assert_eq!(scope::stage_tracked(&temp.0, &staged)?, 3);
    assert_eq!(
        scope::check_utf8(&[staged.join(FIXTURE).join("public-fixture.pfx")])?,
        1
    );
    let original = fs::read(&paths[1])?;
    let record = fs::read(&paths[2])?;
    let mut changed = original.clone();
    let middle = changed.len() / 2;
    changed[middle] ^= 1;
    fs::write(&paths[1], &changed)?;
    let refused = temp.0.join("changed-staging");
    assert!(scope::check_utf8(&paths).is_err());
    assert!(scope::stage_tracked(&temp.0, &refused).is_err());
    assert!(
        !refused.exists(),
        "Changed public fixture was published before validation"
    );
    // Updating a mutable provenance file alongside the payload cannot rewrite
    // the independently reviewed pins embedded in the native staging gate.
    let mut mutable: Value = serde_json::from_slice(&record)?;
    mutable["files"]["public-fixture.pfx"]["sha256"] = ci::sha(&changed).into();
    fs::write(&paths[2], serde_json::to_vec(&mutable)?)?;
    assert!(scope::check_utf8(&paths).is_err());
    assert!(scope::stage_tracked(&temp.0, &refused).is_err());
    fs::write(&paths[1], &original)?;
    fs::write(&paths[2], &record)?;
    let wrong = temp.write("unreviewed/private.pfx", &original)?;
    git(&temp.0, &["add", "--", "unreviewed/private.pfx"])?;
    assert!(scope::check_utf8(&[wrong]).is_err());
    assert!(scope::stage_tracked(&temp.0, &refused).is_err());
    assert!(
        !refused.exists(),
        "Wrong-path signing material entered the scanner exception"
    );
    let suffix = format!("nested/{FIXTURE}/public-fixture.pfx");
    temp.write(&suffix, &original)?;
    temp.write(&format!("nested/{FIXTURE}/provenance.json"), &record)?;
    git(
        &temp.0,
        &["rm", "--cached", "--quiet", "--", "unreviewed/private.pfx"],
    )?;
    git(
        &temp.0,
        &[
            "add",
            "--",
            &suffix,
            &format!("nested/{FIXTURE}/provenance.json"),
        ],
    )?;
    assert!(
        scope::stage_tracked(&temp.0, &refused).is_err(),
        "A duplicate suffix must not become a new reviewed repository path"
    );
    let unexpected = temp.write(&format!("{FIXTURE}/another.p12"), &original)?;
    assert!(scope::check_utf8(&[unexpected]).is_err());
    // Preparing the actual scanner configuration also validates fixture bytes
    // and rejects any change to the reviewed narrow template before mutating
    // the pinned upstream rule source.
    let cfg = Temp::new("secret-template-review")?;
    let cfg_paths = install(&cfg)?;
    let template = fs::read(root().join("scripts/ci/gitleaks.toml"))?;
    assert_eq!(ci::sha(&template), scope::REVIEWED_RULES_SHA256);
    let input = cfg.write("scripts/ci/gitleaks.toml", &template)?;
    let upstream = b"title = \"isolated official-layout control\"\n[[allowlists]]\npaths = [\n'''node_modules'''\n]\n[[rules]]\nid = \"control\"\nregex = '''never-a-real-key'''\n";
    let base = cfg.write("official.toml", upstream)?;
    let output = cfg.0.join("strict.toml");
    scope::prepare_secret_config_checked(&cfg.0, &base, &output, &ci::sha(upstream))?;
    assert!(!fs::read_to_string(&base)?.contains("node_modules"));
    let prepared = fs::read_to_string(output)?;
    assert!(prepared.contains("condition = \"AND\""));
    assert!(!prepared.contains("useDefault = true"));
    for damaged in [
        String::from_utf8(template.clone())?.replace("condition = \"AND\"", "condition = \"OR\""),
        String::from_utf8(template.clone())? + "\n[[allowlists]]\npaths=['''.*''']\n",
    ] {
        fs::write(&input, damaged)?;
        fs::write(&base, upstream)?;
        let refused = cfg.0.join("refused.toml");
        assert!(
            scope::prepare_secret_config_checked(&cfg.0, &base, &refused, &ci::sha(upstream))
                .is_err()
        );
        assert!(!refused.exists());
        assert_eq!(fs::read(&base)?, upstream);
    }
    fs::write(&input, &template)?;
    fs::write(&cfg_paths[1], &changed)?;
    let refused = cfg.0.join("refused-fixture.toml");
    assert!(
        scope::prepare_secret_config_checked(&cfg.0, &base, &refused, &ci::sha(upstream)).is_err()
    );
    assert!(!refused.exists());
    assert_eq!(fs::read(&base)?, upstream);
    Ok(())
}
