use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("Git is required to embed the exact build source");
    assert!(
        output.status.success(),
        "Could not resolve the build source"
    );
    let sha = String::from_utf8(output.stdout).expect("Build source must be UTF8");
    let sha = sha.trim();
    assert!(
        sha.len() == 40 && sha.bytes().all(|value| value.is_ascii_hexdigit()),
        "Invalid build source SHA"
    );
    if let Ok(expected) = env::var("GITHUB_SHA") {
        assert_eq!(
            expected, sha,
            "Build checkout differs from its hosted source"
        );
    }
    println!("cargo:rerun-if-env-changed=NUMERA_ACCEPTANCE_GENERATION");
    println!("cargo:rerun-if-env-changed=NUMERA_ACCEPTANCE_KEY");
    let generation = env::var("GITHUB_RUN_ID").unwrap_or_else(|_| "0".into());
    let mut number: u64 = generation.parse().expect("Invalid build generation");
    if env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        assert!(number > 0, "Hosted builds require their actual generation");
    }
    if env::var_os("CARGO_FEATURE_UPDATE_ACCEPTANCE").is_some() {
        let key =
            env::var("NUMERA_ACCEPTANCE_KEY").expect("Acceptance builds require a fixture key");
        assert!(
            key.len() == 64
                && key
                    .bytes()
                    .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)),
            "Invalid fixture public key"
        );
        assert_ne!(
            key, "d4ab0a9e265f108225bc28089cbcba0823beaad39a3639574c1a56e416dd2fad",
            "Acceptance builds must never use the owner signing key"
        );
        let fixture: u64 = env::var("NUMERA_ACCEPTANCE_GENERATION")
            .expect("Acceptance build generation missing")
            .parse()
            .expect("Invalid fixture generation");
        assert!(
            number > 0
                && number
                    .checked_mul(2)
                    .is_some_and(|base| fixture == base || base.checked_add(1) == Some(fixture)),
            "Invalid hosted fixture generation"
        );
        number = fixture;
    } else {
        assert!(
            env::var_os("NUMERA_ACCEPTANCE_GENERATION").is_none()
                && env::var_os("NUMERA_ACCEPTANCE_KEY").is_none(),
            "Fixture overrides are forbidden in a production build"
        );
    }
    println!("cargo:rustc-env=NUMERA_BUILD_SOURCE_SHA={sha}");
    println!("cargo:rustc-env=NUMERA_BUILD_GENERATION={number}");
    tauri_build::build()
}
