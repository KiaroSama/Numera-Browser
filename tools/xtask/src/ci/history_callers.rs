//! Pinned caller-only adaptations for APIs introduced after a frozen defect.
//! Selected production defects remain their exact Git blobs. Every adaptation
//! requires the original named semantic failure and byte-restored positive.
use super::{
    Result, fail,
    history::{Case, Source},
    history_compat::{Replacement, caller_replacement, checked_caller},
};
use quote::ToTokens;
use std::{fs, path::Path};

const HTTP: &str = "engine-rust/src/http/profile_routes.rs";
const CLOCK_FIXTURE: &str = "engine-rust/tests/profile_read_parity.rs";
const CLOCK_CASE: &str = "relative_activity_http_matches_original_without_rewriting_storage";
const CLOCK_SOURCE: &str = "2077705d1d09bc686361406d68b65ac99de9a049c6957bbd12974237c25935cd";
const OLD_CLOCK_CALL: &str = "numera_core::http::profile_routes::router_with_clock(\n                root,\n                \"fixture-api\",\n                \"fixture-host\",\n                anchor as f64,\n            )?";
const LEGACY_CLOCK_CALL: &str = "numera_core::http::profile_routes::router(\n                root,\n                \"fixture-api\",\n                \"fixture-host\",\n            )?";

/// The first vector's bool-to-numeric admission is independent of wall time.
/// Requiring its actual500/expected200 excludes unrelated clocks/auth/errors.
pub fn negative_evidence(case: &Case) -> super::evidence::Failure {
    if case.target == "profile_read_parity"
        && case.test == CLOCK_CASE
        && case
            .files
            .get(HTTP)
            .is_some_and(|hash| hash == CLOCK_SOURCE)
    {
        super::evidence::Failure::new(
            &case.test,
            "Relative activity HTTP status differs: \"true\"",
        )
        .with_values("Number(500)", "Number(200)")
    } else {
        super::evidence::Failure::new(&case.test, &case.symptom)
    }
}

pub fn clock_caller(case: &Case, current: &[u8]) -> Result<Option<Replacement>> {
    let sources = super::history::case_sources(case)?;
    let Some(source) = sources.get(HTTP).filter(|source| {
        [
            CLOCK_SOURCE,
            "9c3fcf9caf0bc6fa97ba48b62341b527fb1bc6f017e3859720a257b8d88bd202",
        ]
        .contains(&source.sha256.as_str())
    }) else {
        return Ok(None);
    };
    if case.target != "profile_read_parity" {
        return Ok(None);
    }
    if case.files.contains_key(CLOCK_FIXTURE)
        || case.companions.contains_key(CLOCK_FIXTURE)
        || super::sha(current) != "3acba3af4623ae93c68fee443ca03510295235da19288da41e648a45a135ade3"
    {
        return fail("Historical clock caller changed; review its complete matrix and assertions");
    }
    let mut replacement = caller_replacement(
        case,
        CLOCK_FIXTURE,
        current,
        source,
        &[(OLD_CLOCK_CALL, LEGACY_CLOCK_CALL)],
    )?;
    replacement.receipt["historical_source_path"] = HTTP.into();
    replacement.receipt["reason"] = "Old router predates the later frozen-clock observer. Keep all24 fixture inputs, complete payload/status assertions and storage byte checks unchanged; the selected bool timestamp status defect is independent of clock.".into();
    replacement.receipt["clock_scope"] = "Historical router uses real wall time; restored current router uses the original exact frozen clock.".into();
    if case.test == CLOCK_CASE {
        replacement.receipt["required_negative"] = "First true timestamp vector: actual500 versus expected200 at the named status assertion; later body/time mismatches cannot count.".into();
    } else {
        replacement.receipt["clock_fixture_executed"] = false.into();
    }
    Ok(Some(replacement))
}

pub fn additional(root: &Path, case: &Case) -> Result<Vec<(String, Replacement)>> {
    let sources = super::history::case_sources(case)?;
    let mut result = Vec::new();
    if let Some(source) = sources
        .get("engine-rust/src/vault/recovery.rs")
        .filter(|source| {
            source.sha256 == "a86dd9cb60e15f03b20fd0d56e4e1e1767df08cc40746b949a75f432a6b6512b"
        })
    {
        result.push(recovery_transaction(root, case, source)?);
    }
    if let Some(source) = sources
        .get("engine-rust/src/ownership/tree.rs")
        .filter(|source| {
            source.sha256 == "2688b722ab3411a1a5528335a69b029a97e840cb79f7cb065d55735cefe6078b"
        })
    {
        let path = "engine-rust/src/ownership/session.rs";
        let session_hash = super::digest(&root.join(path))?;
        // The separately retained checkpoint already calls the public API and
        // retains admission on every error. Its exact bytes need no bridge.
        if session_hash == "f0f57076e1a2e0d1d6c98e36b3212e08261447c6cbb5b9c382154860c0a94145" {
            checked_caller(root, case, path, &session_hash)?;
        } else {
            let bytes = checked_caller(
                root,
                case,
                path,
                "c35c543c0a84f47c8567ea32a921840ab1bd69a8592c0e8227fa74f44d71737d",
            )?;
            let before = r#"        self.tree = Some(match OwnedTree::spawn_classified(command) {
            Ok(tree) => tree,
            Err(failure) => {
                // Spawn failures may retain an uncertain tree in capture quarantine.
                // A positive no-child/empty proof permits the original owner
                // to release admission; error kinds alone never provide it.
                if !failure.settled
                    && let Some(admission) = &mut self.admission
                {
                    admission.retain();
                }
                return Err(failure.error);
            }
        });"#;
            let after = r#"        self.tree = Some(match OwnedTree::spawn(command) {
            Ok(tree) => tree,
            Err(error) => {
                // The exact old tree API has no failure-stage proof. Retain
                // admission conservatively for every error, regardless of kind.
                if let Some(admission) = &mut self.admission {
                    admission.retain();
                }
                return Err(error);
            }
        });"#;
            let mut replacement =
                caller_replacement(case, path, &bytes, source, &[(before, after)])?;
            replacement.receipt["historical_source_path"] =
                "engine-rust/src/ownership/tree.rs".into();
            replacement.receipt["reason"] = "Only the later Session caller reverts to the exact old public spawn API. Every error conservatively retains admission; no error-kind exemption or selected tree mutation is permitted.".into();
            result.push((path.into(), replacement));
        }
    }
    if case.target == "profile_read_parity"
        && sources.contains_key(HTTP)
        && let Some(replacement) = clock_caller(case, &fs::read(root.join(CLOCK_FIXTURE))?)?
    {
        result.push((CLOCK_FIXTURE.into(), replacement));
    }

    if let Some(source) = sources
        .get("engine-rust/src/browser/options.rs")
        .filter(|source| {
            source.sha256 == "48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263"
        })
    {
        let path = "engine-rust/src/browser/launch.rs";
        let bytes = checked_caller(
            root,
            case,
            path,
            "ec90ea5ae635ff2f2b3188f60b7a74356ff3e25d034ac1d19f299a482050fc24",
        )?;
        let options = fs::read(root.join("engine-rust/src/browser/options.rs"))?;
        if super::sha(&options)
            != "1719e075377820670a5937bfc818cf5768d9692b5e638a2800d0e5ce9fc82e35"
        {
            return fail("Registered options wrapper changed");
        }
        let mut replacement = caller_replacement(
            case,
            path,
            &bytes,
            source,
            &[(
                "Some(driver) => options::build_registered(\n            &profile,\n            driver,",
                "Some(driver) => options::build(\n            &profile,\n            driver.family().options_backend(),",
            )],
        )?;
        replacement.receipt["historical_source_path"] = "engine-rust/src/browser/options.rs".into();
        replacement.receipt["included_source_sha256"] = super::sha(&options).into();
        replacement.receipt["included_source_path"] = "engine-rust/src/browser/options.rs".into();
        replacement.receipt["reason"] = "Inline only the exact current build_registered wrapper's call to build(...driver.family().options_backend()); every argument, option and ownership path stays unchanged.".into();
        result.push((path.into(), replacement));
    }
    if let Some(source) = sources
        .get("engine-rust/src/mcp_command.rs")
        .filter(|source| {
            source.sha256 == "212a821995a0582c0f07913ec1ac9c83172b0fcb1024ccfc1eb3fe9eb95e7dd1"
        })
    {
        let path = "engine-rust/src/http/acquisition_routes.rs";
        let bytes = checked_caller(
            root,
            case,
            path,
            "45a227205898c2c201dc165d3619c23434b58c5ccc55695468eba494b1c5a1dd",
        )?;
        let current = fs::read(root.join("engine-rust/src/mcp_command.rs"))?;
        if super::sha(&current)
            != "192b83bfdaee8b3a181ec7aa28adb42417e87f9ad0181ed3ac706d3e5be21a9d"
        {
            return fail("Current finite child-command preparation changed");
        }
        let parsed = syn::parse_file(std::str::from_utf8(&current)?)?;
        let fragment = parsed
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(function) if function.sig.ident == "child_command" => {
                    Some(function.to_token_stream().to_string())
                }
                _ => None,
            })
            .ok_or("Current child-command preparation absent")?;
        let before = std::str::from_utf8(&bytes)?;
        if before.matches("crate::mcp_command::child_command(").count() != 1 {
            return fail("Acquisition caller command boundary changed");
        }
        let changed = format!(
            "{}\nmod historical_child_preparation {{ use std::{{path::Path, process::Command}}; {fragment} }}\n",
            before.replace(
                "crate::mcp_command::child_command(",
                "historical_child_preparation::child_command("
            )
        );
        let mut replacement =
            caller_replacement(case, path, &bytes, source, &[(before, &changed)])?;
        replacement.receipt["historical_source_path"] = "engine-rust/src/mcp_command.rs".into();
        replacement.receipt["included_source_sha256"] = super::sha(&current).into();
        replacement.receipt["included_source_path"] = "engine-rust/src/mcp_command.rs".into();
        replacement.receipt["included_fragment_sha256"] = super::sha(fragment.as_bytes()).into();
        replacement.receipt["reason"] = "Keep the frozen MCP companion unchanged. The later acquisition caller uses the identical pinned current typed child_command body, including every private environment removal; admission/capture/receipt/cleanup paths remain untouched.".into();
        result.push((path.into(), replacement));
    }
    Ok(result)
}

fn recovery_transaction(
    root: &Path,
    case: &Case,
    source: &Source,
) -> Result<(String, Replacement)> {
    let path = "engine-rust/src/vault/transaction.rs";
    let hash = super::digest(&root.join(path))?;
    if ![
        "09455cb29f59983d127ad06f7d18bfe767f1f673e8f7b3842b01cc430a31b51d",
        // Same callers with the separately reviewed salt/verifier presence
        // guard. All descriptor, admission and data operations remain intact.
        "bf895fd1b47f7281e166285306ab5dc33e9a80da2ff8ec956c1e0a5ed1ff12d1",
    ]
    .contains(&hash.as_str())
    {
        return fail("Current transaction caller changed; review its historical error ABI");
    }
    let bytes = checked_caller(root, case, path, &hash)?;
    let before = std::str::from_utf8(&bytes)?;
    let patches = [
        (
            "recovery::{RecoveryError, caused_refusal, incomplete_for, refusal}",
            "recovery::{RecoveryError, incomplete_for, refusal}",
            1,
        ),
        (
            "caused_refusal(format!(\"Vault salt randomness unavailable: {error}\"), error)",
            "refusal(format!(\"Vault salt randomness unavailable: {error}\"))",
            2,
        ),
        (
            "caused_refusal(error.to_string(), error)",
            "refusal(error.to_string())",
            1,
        ),
        (
            ".map_err(super::recovery::migration_failure)?",
            ".map_err(|error| refusal(error.to_string()))?",
            2,
        ),
        (
            "return Err(super::recovery::rollback_failed(\n                error,\n                failures,\n                \"Vault migration failed\",\n            ));",
            "let rollback = crate::storage::atomic::RollbackFailed { cause: io::Error::other(error), failures };\n            return Err(refusal(format!(\"Vault migration failed: {rollback}; journal retained\")));",
            1,
        ),
    ];
    let mut changed = before.to_owned();
    for (old, new, count) in patches {
        if changed.matches(old).count() != count {
            return fail("Historical transaction error caller shape changed");
        }
        changed = changed.replace(old, new);
    }
    let mut replacement = caller_replacement(case, path, &bytes, source, &[(before, &changed)])?;
    replacement.receipt["historical_source_path"] = "engine-rust/src/vault/recovery.rs".into();
    replacement.receipt["historical_sources"] =
        serde_json::to_value(super::history::case_sources(case)?)?;
    replacement.receipt["caller_rewrites"] = serde_json::to_value(patches)?;
    replacement.receipt["reason"] = "The old RecoveryError stores message/codes/role only. Adapt only later transaction error callers to its refusal constructor, preserving every message argument and the complete current RollbackFailed display. All selected recovery and rotation bytes, test vectors, file operations and descriptor guards remain unchanged.".into();
    replacement.receipt["error_abi_limitation"] = "Newer typed cause-chain fields cannot be represented by the old RecoveryError. They are outside the five exercised config-read vectors. The separately retained caller-only five-vector GREEN, selected exact named RED and fully restored five-vector GREEN distinguish this caller ABI bridge from the selected defect.".into();
    Ok((path.into(), replacement))
}

pub fn companion(
    case: &Case,
    path: &str,
    source: &Source,
    historical: &[u8],
) -> Result<Option<Replacement>> {
    if path != "engine-rust/src/network/proxy_check.rs"
        || source.sha256 != "39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5"
        || case.companions.get(path) != Some(source)
    {
        return Ok(None);
    }
    let mut replacement = caller_replacement(
        case,
        path,
        historical,
        source,
        &[(
            "let url = match Url::parse(&relay.url()) {",
            "let relay_url = match relay.url() {\n            Ok(url) => url,\n            Err(_) => return json!({\"ok\":false,\"error\":\"Native relay URL is invalid\"}),\n        };\n        let url = match Url::parse(&relay_url) {",
        )],
    )?;
    replacement.receipt["reason"] = "Only the old companion's URL caller adapts String to the current fallible Relay::url; retain the exact existing invalid-URL result and ordinary URL parse path. A primary selected proxy-check defect is never eligible.".into();
    Ok(Some(replacement))
}
