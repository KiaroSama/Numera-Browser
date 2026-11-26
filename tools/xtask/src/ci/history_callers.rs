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
const TREE: &str = "engine-rust/src/ownership/tree.rs";
const SESSION: &str = "engine-rust/src/ownership/session.rs";
const SESSION_CLASSIFIED: &str = "c35c543c0a84f47c8567ea32a921840ab1bd69a8592c0e8227fa74f44d71737d";
const MACOS_TREE: &str = "201260e01da09fea13951ba61ab0265faa68499554d4cfefaf712a179dc8b36b";
const SETTINGS: &str = "engine-rust/src/cli/settings_commands.rs";
const CONFIG: &str = "engine-rust/src/storage/config.rs";
const STRICT_SETTINGS_CASE: &str =
    "strict_settings::settings_commands_preserve_original_decoding_refusals_and_early_errors";
const PROXY_CHECK: &str = "engine-rust/src/network/proxy_check.rs";
const PROXY_CHECK_COMMIT: &str = "4e570f130ba807448453c12777457bcf55911a67";
const PROXY_CHECK_SOURCE: &str = "39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5";
const PROXY_ECHO_CASE: &str =
    "network::tests::proxy_echo_boundaries_match_original_through_transport";
const BULK: &str = "engine-rust/src/bulk.rs";
const BULK_SELECTION_CASE: &str =
    "selection::selection_matches_original_complete_profile_membership";
const CLAIMS: &str = "engine-rust/src/ownership/claims.rs";
const CLAIM_DIAGNOSTICS_CASE: &str =
    "diagnostics::claim_diagnostics_match_original_truth_and_display_without_changing_records";
const UNIT_REPORT: &str = "engine-rust/src/ownership/unit_report.rs";
const RECEIPT_TOKENS_CASE: &str =
    "receipt_tokens_match_original_without_resolving_invalid_transfers";
const RUNTIME_LOG: &str = "engine-rust/src/runtime_log.rs";
const LOG_RETENTION_CASE: &str =
    "retention::run_retention_matches_original_mtime_order_and_namespace";
const LOCKS: &str = "engine-rust/src/storage/locks.rs";
const EXTENSION_ICON_CASE: &str =
    "icons::extension_icon_http_matches_original_known_folder_and_file_contracts";

/// The separate owning-host Mac proof may adapt only this later Session
/// caller. Its exact selected tree/test/commit never become a canonical99 row.
pub(super) fn macos_session_caller(
    case: &Case,
    path: &str,
    source: &Source,
    current: &[u8],
) -> bool {
    path == SESSION
        && source.commit == "897caadef85053d6d74896e901fc9a21f7baa6dd"
        && source.sha256 == MACOS_TREE
        && case.files.get(TREE) == Some(&source.sha256)
        && super::history::macos_cases().contains(case)
        && super::sha(current) == SESSION_CLASSIFIED
}
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
    if case.target == "bulk_parity"
        && case.test == BULK_SELECTION_CASE
        && let Some(source) = sources.get(BULK).filter(|source| {
            source.commit == "d7227856f1fc7bec01b66d3737d3cbb729115623"
                && source.sha256
                    == "e425d6fd6a60615996504fb5c58758a895bc574126609650f9fafeee62938d09"
                && case.files.get(BULK) == Some(&source.sha256)
        })
    {
        result.extend(bulk_selection_callers(root, case, source)?);
    }
    if case.target == "claim_parity"
        && case.test == CLAIM_DIAGNOSTICS_CASE
        && let Some(source) = sources.get(CLAIMS).filter(|source| {
            source.commit == "d25e088d8adb4c70e2dd401ffbe100331cfb35f4"
                && source.sha256
                    == "6af8a9ffd4a29f9ce29da25da9ecf8e4b4fc2031b9228424be8c578650eccbb8"
                && case.files.get(CLAIMS) == Some(&source.sha256)
        })
    {
        let bytes = checked_caller(root, case, SESSION, SESSION_CLASSIFIED)?;
        let mut replacement = caller_replacement(
            case,
            SESSION,
            &bytes,
            source,
            &[(
                "claims::release_before(&self.root, &self.id, claim, tree, deadline)?",
                "claims::release(&self.root, &self.id, claim, tree)?",
            )],
        )?;
        replacement.receipt["historical_source_path"] = CLAIMS.into();
        replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
        replacement.receipt["session_cleanup_executed"] = false.into();
        replacement.receipt["reason"] = "The frozen claims API exposes release, so the unexecuted later Session caller uses that signature. The original diagnostic assertions call describe directly and retain the exact selected source.".into();
        result.push((SESSION.into(), replacement));
    }
    if case.target == "ownership_receipt_parity"
        && case.test == RECEIPT_TOKENS_CASE
        && let Some(source) = sources.get(UNIT_REPORT).filter(|source| {
            source.commit == "b1ad79f18f75d9a328b731bb092130cc3b61541a"
                && source.sha256
                    == "0db789c9ab6f184f811bfadd0f1d3cea7ab0b0bd058663eecbffd816a7bbadcb"
                && case.files.get(UNIT_REPORT) == Some(&source.sha256)
        })
    {
        result.push(receipt_tokens_callers(root, case, source)?);
    }
    if case.target == "logging_parity"
        && case.test == LOG_RETENTION_CASE
        && let Some(source) = sources.get(RUNTIME_LOG).filter(|source| {
            source.commit == "3be53623c6dbd55e22ab23dcbe28dc7c4852053d"
                && source.sha256
                    == "78db62948c0519ffadbcf154574caac10078bee0577c6269a0161e4191743590"
                && case.files.get(RUNTIME_LOG) == Some(&source.sha256)
        })
    {
        result.push(log_retention_callers(root, case, source)?);
    }
    if case.target == "extension_routes_parity"
        && case.test == EXTENSION_ICON_CASE
        && case.platform.as_deref() == Some("win32")
        && let Some(source) = sources.get(LOCKS).filter(|source| {
            source.commit == "8354eeb286d93f9418ee372ed057c129521404ad"
                && source.sha256
                    == "8cc9b17a4c77c5db9b178d8ea2b16318132f5d11439359c3e47abca269d6a373"
                && case.files.get(LOCKS) == Some(&source.sha256)
        })
    {
        let path = "engine-rust/src/http/extension_routes.rs";
        let bytes = checked_caller(
            root,
            case,
            path,
            "62a6a1b4cd98564871af73c443a726fc834b00226061a7985d962d89764746fb",
        )?;
        let mut replacement = caller_replacement(
            case,
            path,
            &bytes,
            source,
            &[(
                "let expanded = expand_home(std::path::Path::new(text), &home).map_err(|_| failure())?;",
                "let path = std::path::Path::new(text);\n    let expanded = if path.is_absolute() {\n        path.to_owned()\n    } else {\n        expand_home(path, &home).map_err(|_| failure())?\n    };",
            )],
        )?;
        replacement.receipt["historical_source_path"] = LOCKS.into();
        replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
        replacement.receipt["reason"] = "The archival home expansion returned absolute request paths byte-for-byte. Preserve that caller behavior so the exact frozen lock resolver receives the Windows verbatim parent component.".into();
        result.push((path.into(), replacement));
    }
    if case.target == "lib"
        && case.test == PROXY_ECHO_CASE
        && case
            .files
            .get(PROXY_CHECK)
            .is_some_and(|hash| hash == PROXY_CHECK_SOURCE)
        && let Some(source) = sources
            .get(PROXY_CHECK)
            .filter(|source| source.commit == PROXY_CHECK_COMMIT)
    {
        result.push(proxy_echo_relay(root, case, source)?);
    }
    if case.target == "cli_settings_parity"
        && case.test == STRICT_SETTINGS_CASE
        && let Some(source) = sources.get(SETTINGS).filter(|source| {
            source.commit == "6c8a5bbc56c32ebf611d286422df3a94837abc7a"
                && source.sha256
                    == "b061d6a64dd454340956b51273f09b75a123b4889f35e34cef1d99b073a34b1b"
        })
    {
        result.extend(settings_home(root, case, source)?);
        let config = sources
            .get(CONFIG)
            .ok_or("Selected historical settings config is missing")?;
        result.push(settings_fixture(root, case, config)?);
    }
    if let Some(source) = sources
        .get("engine-rust/src/vault/recovery.rs")
        .filter(|source| {
            source.sha256 == "a86dd9cb60e15f03b20fd0d56e4e1e1767df08cc40746b949a75f432a6b6512b"
        })
    {
        result.push(recovery_transaction(root, case, source)?);
    }
    if let Some(source) = sources.get(TREE).filter(|source| {
        [
            "2688b722ab3411a1a5528335a69b029a97e840cb79f7cb065d55735cefe6078b",
            MACOS_TREE,
        ]
        .contains(&source.sha256.as_str())
    }) {
        let path = SESSION;
        let session_hash = super::digest(&root.join(path))?;
        // The separately retained checkpoint already calls the public API and
        // retains admission on every error. Its exact bytes need no bridge.
        if source.sha256 != MACOS_TREE
            && session_hash == "f0f57076e1a2e0d1d6c98e36b3212e08261447c6cbb5b9c382154860c0a94145"
        {
            checked_caller(root, case, path, &session_hash)?;
        } else {
            let bytes = checked_caller(root, case, path, SESSION_CLASSIFIED)?;
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
            != "a983386b95f5ce5edd9cbad99841a96eaf3430c22263ff8544c3cd83ae027365"
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
    result.extend(super::history_models::additional(root, case, &sources)?);
    Ok(result)
}

fn receipt_tokens_callers(
    root: &Path,
    case: &Case,
    source: &Source,
) -> Result<(String, Replacement)> {
    let path = "engine-rust/src/ownership/mod.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "74ab71be3892332d2ed10d1432dd0db9fb4b9e29cf27fe1abdf45b414cecd757",
    )?;
    let current = fs::read(root.join(UNIT_REPORT))?;
    let current_sha = super::sha(&current);
    if current_sha != "ec8ad2618dd65545d84af424f83de3e1deaa14586e7478bb3d8964d92898fbfb" {
        return fail("Current receipt transfer and diagnostic support changed");
    }
    let (support, _) = std::str::from_utf8(&current)?
        .split_once("\npub fn for_job(")
        .ok_or("Current receipt support boundary changed")?;
    let wrapper = format!(
        r#"#[path = "unit_report.rs"]
mod frozen_unit_report;
#[allow(dead_code)]
mod later_receipt_support {{
{support}
}}
pub mod unit_report {{
    pub use super::frozen_unit_report::*;
    pub use super::later_receipt_support::diagnostic;
    pub(crate) use super::later_receipt_support::{{SpawnIntent, begin_transfer}};
}}"#
    );
    let mut replacement = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[("pub mod unit_report;", &wrapper)],
    )?;
    replacement.receipt["historical_source_path"] = UNIT_REPORT.into();
    replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
    replacement.receipt["included_source_sha256"] = current_sha.into();
    replacement.receipt["included_fragment_sha256"] = super::sha(support.as_bytes()).into();
    replacement.receipt["reason"] = "Only the later transfer capability and diagnostic symbols come from current source. The original fixture's read, read_scoped and record_at use the exact frozen module.".into();
    Ok((path.into(), replacement))
}

fn log_retention_callers(
    root: &Path,
    case: &Case,
    source: &Source,
) -> Result<(String, Replacement)> {
    let path = "engine-rust/src/lib.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "491eae3f34700f27c81c67934254310b5e6a3f56db38df4f7e9084410736ae02",
    )?;
    let current = fs::read(root.join(RUNTIME_LOG))?;
    let current_sha = super::sha(&current);
    if current_sha != "c45bbf5f6794ea73d0f31095354762dcb5e3f06806462cef2d71732090b8ad1f" {
        return fail("Current fileless runtime log constructor changed");
    }
    let (_, start) = std::str::from_utf8(&current)?
        .split_once("\n    pub(crate) fn without_file() -> Self {")
        .ok_or("Current fileless runtime log constructor is absent")?;
    let (body, _) = start
        .split_once("\n    pub(crate) fn directory(")
        .ok_or("Current fileless runtime log constructor boundary changed")?;
    let constructor = format!("    pub(crate) fn without_file() -> Self {{{body}");
    let wrapper = format!(
        "pub mod runtime_log {{\n    include!(\"runtime_log.rs\");\n    impl RuntimeLog {{\n{constructor}\n    }}\n}}"
    );
    let mut replacement = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[("pub mod runtime_log;", &wrapper)],
    )?;
    replacement.receipt["historical_source_path"] = RUNTIME_LOG.into();
    replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
    replacement.receipt["included_source_sha256"] = current_sha.into();
    replacement.receipt["included_fragment_sha256"] = super::sha(constructor.as_bytes()).into();
    replacement.receipt["reason"] = "The original retention fixture opens files through the exact frozen module. Only later recovery callers use the current fileless constructor.".into();
    Ok((path.into(), replacement))
}

fn bulk_selection_callers(
    root: &Path,
    case: &Case,
    source: &Source,
) -> Result<Vec<(String, Replacement)>> {
    [
        (
            "engine-rust/src/cli/bulk_commands.rs",
            "637dec32286110ff8602b831da46ac9765276bbf989f21b9e1c4e4a01e550026",
            r#"start: whole(args, "start", 1)?,"#,
            r#"start: i64::try_from(whole(args, "start", 1)?).map_err(|_| {
                    RepoError::Message("start is outside the native supported range".into())
                })?,"#,
        ),
        (
            "engine-rust/tests/bulk_parity/integers.rs",
            "01c7baa36287ff3f13a0ea8c968a71173b4cfd1327a3c8eb3a84cbca67f00e05",
            ".parse::<num_bigint::BigInt>()?,",
            ".parse::<num_bigint::BigInt>()?.try_into()?,",
        ),
    ]
    .into_iter()
    .map(|(path, digest, before, after)| {
        let bytes = checked_caller(root, case, path, digest)?;
        let mut replacement = caller_replacement(case, path, &bytes, source, &[(before, after)])?;
        replacement.receipt["historical_source_path"] = BULK.into();
        replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
        replacement.receipt["generation_executed"] = false.into();
        replacement.receipt["reason"] = "The unselected generation callers use checked i64 conversion for the old GenerateBatch ABI. The frozen bulk module and original selection assertions stay exact.".into();
        Ok((path.into(), replacement))
    })
    .collect()
}

fn settings_home(root: &Path, case: &Case, source: &Source) -> Result<Vec<(String, Replacement)>> {
    const DATA_ROOT: &str = "engine-rust/src/storage/data_root.rs";
    const EXTENSIONS: &str = "engine-rust/src/browser/extensions.rs";
    const OLD_DATA_ROOT: &str = "df4c60971bd5dc58e37bed5095ea5d5281f5468a103680c4183897bb20514dbb";
    let historical = super::git(root, &["show", &format!("{}:{DATA_ROOT}", source.commit)])?;
    if super::sha(&historical) != OLD_DATA_ROOT {
        return fail("Historical settings home resolver differs from its reviewed source");
    }
    let historical = std::str::from_utf8(&historical)?;
    let start = historical
        .find("pub(crate) fn expand_home(")
        .ok_or("Historical settings home resolver is missing")?;
    let (function, _) = historical[start..]
        .split_once("\n\nfn writable(")
        .ok_or("Historical settings home resolver boundary differs")?;
    let data_root = checked_caller(
        root,
        case,
        DATA_ROOT,
        "8511ae2d912996eb3b68e6d45dceaeb22fa375fbc210e736d7dbf925bd031f06",
    )?;
    let extensions = checked_caller(
        root,
        case,
        EXTENSIONS,
        "f62e2096699adff20b82ddea0be8cc06b2cab903ef9c82e307c75873a5b44bf7",
    )?;
    let mut data_root = caller_replacement(
        case,
        DATA_ROOT,
        &data_root,
        source,
        &[
            (
                "pub(crate) fn expand_home(path: &Path, home: &Path) -> io::Result<PathBuf> {\n    super::home_path::expand(path, home)\n}",
                function,
            ),
            (
                "return expand_home(path, home);",
                "return super::home_path::expand(path, home);",
            ),
        ],
    )?;
    let mut extensions = caller_replacement(
        case,
        EXTENSIONS,
        &extensions,
        source,
        &[("data_root::expand_home", "home_path::expand as expand_home")],
    )?;
    for bridge in [&mut data_root, &mut extensions] {
        bridge.receipt["historical_source_path"] = SETTINGS.into();
        bridge.receipt["included_source_path"] = DATA_ROOT.into();
        bridge.receipt["included_source_sha256"] = OLD_DATA_ROOT.into();
        bridge.receipt["included_fragment_sha256"] = super::sha(function.as_bytes()).into();
        bridge.receipt["reason"] = "Give the selected historical settings caller its snapshot's exact infallible home resolver. Later data-root and browser callers keep the current fallible resolver; all selected source and assertion bytes remain exact.".into();
    }
    Ok(vec![
        (DATA_ROOT.into(), data_root),
        (EXTENSIONS.into(), extensions),
    ])
}

fn settings_fixture(root: &Path, case: &Case, source: &Source) -> Result<(String, Replacement)> {
    let path = "engine-rust/tests/cli_settings_parity/named_home.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "09b60d9be3dbc3daa35b348e3b687a06eb1b80231933aa3cf60d8dbdd8338858",
    )?;
    let mut replacement = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[
            (
                "assert!(!config.headless_launch()?);",
                "assert!(!config.headless_launch());",
            ),
            (
                "cli::settings_commands::windows_named_home(",
                "numera_core::storage::home_path::windows_named_home(",
            ),
        ],
    )?;
    replacement.receipt["historical_source_path"] = CONFIG.into();
    replacement.receipt["historical_settings_source_path"] = SETTINGS.into();
    replacement.receipt["fixture_executed"] = false.into();
    replacement.receipt["reason"] = "The later named-home sibling fixture uses the selected config snapshot's bool return type and calls the same Windows home policy directly, since the selected settings snapshot predates its forwarding helper. The original strict-settings assertion runs as registered.".into();
    Ok((path.into(), replacement))
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
        "881a313635afe3d8242a3f4d38af84fc96efc70a8e824f9b5e671407eb97e5e1",
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

fn proxy_echo_relay(root: &Path, case: &Case, source: &Source) -> Result<(String, Replacement)> {
    let path = "engine-rust/src/network.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "ef36a2955f4e306612f8abae4ff92fde4539827867de6177fbace475accb7499",
    )?;
    let wrapper = r#"mod historical_proxy_echo {
    use super::{builder, failure, get};

    mod relay {
        pub(super) struct Relay(super::super::relay::Relay);

        impl Relay {
            pub(super) fn start(proxy: &crate::proxy::Proxy) -> std::io::Result<Self> {
                super::super::relay::Relay::start(proxy).map(Self)
            }

            pub(super) fn url(&self) -> String {
                self.0.url().unwrap_or_default()
            }

            pub(super) fn last_error(&self) -> Option<String> {
                self.0.last_error()
            }

            pub(super) fn close(&mut self, budget: std::time::Duration) -> std::io::Result<()> {
                self.0.close(budget)
            }
        }
    }

    pub mod proxy_check {
        include!("network/proxy_check.rs");
    }
}
pub use historical_proxy_echo::proxy_check;"#;
    let mut replacement = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[("pub mod proxy_check;", wrapper)],
    )?;
    replacement.receipt["historical_source_path"] = PROXY_CHECK.into();
    replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
    replacement.receipt["reason"] = "Only the selected older caller sees the String-returning relay URL ABI. Relay startup, ownership, errors and cleanup delegate to the current implementation. An unavailable URL follows the original invalid-URL branch. The selected proxy check and original test remain exact.".into();
    Ok((path.into(), replacement))
}

pub fn companion(
    case: &Case,
    path: &str,
    source: &Source,
    historical: &[u8],
) -> Result<Option<Replacement>> {
    if path != PROXY_CHECK
        || source.sha256 != PROXY_CHECK_SOURCE
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
