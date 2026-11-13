//! Frozen source identities and actual native execution evidence. A maintained
//! name must be declared, applicable on its reviewed platform, and reported ok.
use super::{Result, decode, fail, sha};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    rc::Rc,
};
use syn::{
    Attribute, Expr, Item, Lit, LitStr, Meta, Token,
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
};

const SOURCE_COMMIT: &str = "3bf1d839fd33e42a10b982bd8607a7052ea7c4cd";
const SOURCE_IDS_SHA: &str = "cc5729186a0a8c9355ddbe12d69894bb5a425b12573a9ba948a14c4419f9872d";
const NATIVE_NAMES_SHA: &str = "9c27715f4a6b62f72bfbd37b310b1abc82567548fc51b424bd07b630010e297c";
const LIVE: &str = "selected_sdk_opens_real_persistent_context_in_native_owned_tree";
const ALL: u8 = 7;
const MAPS: &[&str] = &[
    "source-acceptance-test-map.json",
    "source-browser-test-map.json",
    "source-ci-test-map.json",
    "source-cloak-test-map.json",
    "source-contracts-test-map.json",
    "source-domain-test-map.json",
    "source-http-test-map.json",
    "source-network-test-map.json",
    "source-ownership-test-map.json",
    "source-performance-test-map.json",
    "source-storage-test-map.json",
    "source-vault-test-map.json",
];
// These eight existing native cfgs are part of the reviewed original inventory.
// They may not be widened/narrowed by changing a map or a current #[cfg].
const NATIVE_PLATFORM_POLICY: &[(&str, u8)] = &[
    (
        "job_spawn::tests::cancelled_handoff_settles_actual_supervisor_and_command",
        1,
    ),
    (
        "ownership::tree::tests::malformed_group_observations_match_original_without_releasing_claims",
        1,
    ),
    (
        "imported_attributes_match_original_without_replacing_destination_metadata",
        1,
    ),
    (
        "direct_copy_access_times_match_original_without_changing_copytree_snapshots",
        1,
    ),
    (
        "fifo_import_matches_original_refusal_continuation_and_retry",
        3,
    ),
    (
        "copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry",
        3,
    ),
    (
        "profile_count::advisory_profile_count_matches_original_when_destination_scan_is_denied",
        3,
    ),
    (
        "ownership::update_admission_tests::linked_update_parent_or_work_directory_never_becomes_an_admission_path",
        3,
    ),
];
// The294-name baseline was collected on Linux. These five original macOS-only
// tests also exist verbatim in the immutable3bf checkpoint (job_supervisor.rs
// and ownership/macos_tests.rs); preserving platforms must preserve their IDs.
// They are original platform cases, not supplementary tests added by the port.
const ORIGINAL_MACOS_NATIVE: &[&str] = &[
    "job_supervisor::tests::unsupported_detach_refuses_before_files_or_processes",
    "ownership::macos::tests::macos_birth_identity_refuses_malformed_records_and_reused_tokens",
    "ownership::macos::tests::macos_group_growth_is_bounded_and_never_calls_truncation_empty",
    "ownership::macos::tests::macos_public_group_observation_retains_every_uncertain_claim",
    "ownership::macos::tests::macos_public_identity_distinguishes_absence_from_observation_failure",
];
const SOURCE_PLATFORM_POLICY: &[(&str, u8)] = &[
    (
        "tests/test_owned_tree.py::test_a_group_of_corpses_is_not_a_running_browser",
        3,
    ),
    (
        "tests/test_owned_tree.py::test_a_job_that_could_not_be_identified_refuses_to_stop",
        3,
    ),
    (
        "tests/test_owned_tree.py::test_every_kernel32_call_is_bound_and_the_library_is_one_object",
        4,
    ),
    (
        "tests/test_owned_tree.py::test_the_tree_is_not_the_engines_own_process_group",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_bad_stat_text_is_unknown_not_a_zombie",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0--True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 ? 123\\n-True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 R<s 123\\n-True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z 123\\n123 S 456\\n-True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z 123\\n987 S 987\\n-False-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-123 Z+ 123\\n123 Z 456\\n-False-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-0-malformed\\n-True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-1-987 S 987\\n-True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-1-None--True-1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-3-0-123 S 123\\n-False-0]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[darwin-5-0-123 Z 123\\n-True-0]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_darwin_denied_probe_requires_complete_zombie_aware_observation[linux-1-0-123 Z 123\\n-True-0]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_delivered_group_signal_is_recorded_without_changing_ownership",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_disappearing_procfs_entry_is_not_an_observation_error",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_end_tree_cannot_report_a_live_child_stopped_from_failed_observation",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_failed_probe_invalidates_a_previous_live_process_sample",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_healthy_running_and_reaped_child_controls",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ignored_pid_is_excluded_before_its_stat_is_read",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_probe_disappearance_and_permission_controls[13-True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_probe_disappearance_and_permission_controls[3-False]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_process_table_listing_failure_is_unknown",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_failed_query_diagnostic_never_emits_raw_output",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_failure_diagnostic_is_bounded_numeric_and_does_not_query_again",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0--True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 S 0\\n-True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 S 123\\n-True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 Z 123\\n-False]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-123 Z+ 123\\n-False]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-987 S 987\\n-False]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[0-invalid-row\\n-True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_ps_fallback_checks_exit_and_row_integrity[1-123 S 123\\n-True]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable[13]",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_real_running_child_is_not_declared_gone_when_its_stat_is_unreadable[5]",
        1,
    ),
    (
        "tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[1-signal_errno=1]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[3-signal_errno=3]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[identity-ownership_mismatch]",
        3,
    ),
    (
        "tests/test_posix_observation_errors.py::test_signal_refusal_diagnostic_reuses_the_attempt_without_extra_queries[own-group-caller_group]",
        3,
    ),
    (
        "tests/test_session_ownership.py::test_a_child_that_cannot_be_put_in_its_job_is_stopped",
        4,
    ),
    (
        "tests/test_session_ownership.py::test_a_launcher_that_never_led_a_group_still_owns_its_browser",
        3,
    ),
    (
        "tests/test_session_ownership.py::test_a_session_that_cannot_be_owned_is_not_spawned",
        4,
    ),
    (
        "tests/test_session_ownership.py::test_a_tree_that_cannot_be_taken_leaves_the_host_where_it_was",
        3,
    ),
    (
        "tests/test_session_ownership.py::test_stop_reports_a_session_whose_launcher_is_gone",
        3,
    ),
    (
        "tests/test_session_tree_mandatory.py::test_a_driver_that_cannot_be_put_in_a_job_is_stopped",
        4,
    ),
    (
        "tests/test_session_tree_mandatory.py::test_a_job_that_cannot_be_created_refuses_before_anything_starts",
        4,
    ),
    (
        "tests/test_shell_death_cleanup.py::test_a_killed_shell_closes_the_browsers_the_engine_launched",
        3,
    ),
];

#[derive(Clone, Debug)]
struct Case {
    mask: u8,
}
#[derive(Clone, Debug)]
pub struct Inventory {
    targets: BTreeMap<(String, String), BTreeMap<String, Case>>,
}
fn crate_key(name: &str) -> Result<&'static str> {
    match name {
        "engine-rust" | "numera-core" | "numera_core" => Ok("engine-rust"),
        "tools/xtask" | "numera-xtask" | "numera_xtask" => Ok("tools/xtask"),
        _ => fail("Unknown maintained test crate"),
    }
}
fn platform_bit(platform: &str) -> Result<u8> {
    match platform {
        "linux" => Ok(1),
        "macos" | "darwin" => Ok(2),
        "windows" | "win32" => Ok(4),
        _ => fail("Unknown maintained test platform"),
    }
}
impl Inventory {
    pub fn applicable(&self, crate_name: &str, target: &str, platform: &str) -> Result<bool> {
        let bit = platform_bit(platform)?;
        let rows = self
            .targets
            .get(&(crate_key(crate_name)?.into(), target.into()))
            .ok_or("Unknown maintained test target")?;
        Ok(rows.values().any(|case| case.mask & bit != 0))
    }
    pub fn expected(
        &self,
        crate_name: &str,
        target: &str,
        platform: &str,
    ) -> Result<BTreeSet<String>> {
        let bit = platform_bit(platform)?;
        let target = if crate_key(crate_name)? == "engine-rust" && target == "numera_core" {
            "lib"
        } else {
            target
        };
        let rows = self
            .targets
            .get(&(crate_key(crate_name)?.into(), target.into()))
            .ok_or("Unknown maintained test target")?;
        let names = rows
            .iter()
            .filter(|(_, case)| case.mask & bit != 0)
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();
        if names.is_empty() {
            return fail(
                "Test target has no applicable maintained cases; do not execute a zero-case target",
            );
        }
        Ok(names)
    }
    pub fn native_expected(&self, platform: &str) -> Result<BTreeSet<String>> {
        let bit = platform_bit(platform)?;
        let mut names = BTreeSet::new();
        for ((crate_name, target), rows) in &self.targets {
            if crate_name != "engine-rust" || target == "browser_adapter_live" {
                continue;
            }
            for (name, case) in rows {
                if case.mask & bit != 0 && !names.insert(name.clone()) {
                    return fail("Full native report contains ambiguous cross-target names");
                }
            }
        }
        if names.is_empty() {
            return fail("Native case inventory is empty");
        }
        Ok(names)
    }
}
fn strings(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or("Frozen names are not an array")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| std::io::Error::other("Frozen name is not text").into())
        })
        .collect()
}
fn frozen(baseline: &Value) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let source = strings(&baseline["source_test_cases"]["python"])?;
    let native = strings(&baseline["native_reported_test_names"])?;
    if baseline["source_commit"] != SOURCE_COMMIT
        || source.len() != 3418
        || native.len() != 294
        || baseline["native_reported_test_count"] != 294
        || sha(source.join("\n").as_bytes()) != SOURCE_IDS_SHA
        || sha(native.join("\n").as_bytes()) != NATIVE_NAMES_SHA
    {
        return fail("Frozen3418 source /294 native identity inventory differs");
    }
    let source_set = source.into_iter().collect::<BTreeSet<_>>();
    let mut native_set = native.into_iter().collect::<BTreeSet<_>>();
    if source_set.len() != 3418 || native_set.len() != 294 {
        return fail("Frozen identity inventory contains duplicates");
    }
    for name in ORIGINAL_MACOS_NATIVE {
        if !native_set.insert((*name).to_owned()) {
            return fail("Original macOS identity duplicates the Linux native baseline");
        }
    }
    Ok((source_set, native_set))
}
fn source_mask(id: &str) -> u8 {
    SOURCE_PLATFORM_POLICY
        .iter()
        .find(|(name, _)| *name == id)
        .map_or(ALL, |(_, mask)| *mask)
}
fn native_mask(name: &str) -> u8 {
    if ORIGINAL_MACOS_NATIVE.contains(&name) {
        return 2;
    }
    NATIVE_PLATFORM_POLICY
        .iter()
        .find(|(id, _)| *id == name)
        .map_or(ALL, |(_, mask)| *mask)
}
fn safe_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || path.extension().is_none_or(|s| s != "rs")
    {
        return fail("Mapped Rust source path is unsafe");
    }
    let path = root.join(path);
    if fs::symlink_metadata(&path)?.file_type().is_symlink()
        || !path.is_file()
        || !path.canonicalize()?.starts_with(root.canonicalize()?)
    {
        return fail("Mapped Rust source is linked, missing, or outside the repository");
    }
    Ok(path)
}
fn source_target(file: &str) -> Result<(String, String)> {
    let prefix = if file.starts_with("engine-rust/tests/") {
        "engine-rust"
    } else if file.starts_with("tools/xtask/tests/") {
        "tools/xtask"
    } else {
        return fail("Source-case test is outside a maintained integration target");
    };
    let tail = file
        .strip_prefix(&format!("{prefix}/tests/"))
        .ok_or("Test path prefix is invalid")?;
    let target = tail
        .split('/')
        .next()
        .ok_or("Test target missing")?
        .trim_end_matches(".rs");
    if !target.starts_with("source_") {
        return fail("Original source case is mapped to a broad legacy target");
    }
    Ok((prefix.into(), target.into()))
}

#[derive(Clone)]
struct Declaration {
    file: String,
    mask: u8,
    ignored: u8,
    source: Option<String>,
}
#[derive(Clone, Copy)]
struct Location<'a> {
    crate_name: &'a str,
    target: &'a str,
    prefix: &'a [String],
    path: &'a Path,
}
struct Walk<'a> {
    location: Location<'a>,
    directory: &'a Path,
    mask: u8,
    depth: usize,
}
struct Scanner<'a> {
    root: &'a Path,
    files: BTreeMap<PathBuf, Rc<syn::File>>,
    declarations: BTreeMap<(String, String, String), Declaration>,
}
fn literal(expr: &Expr) -> Option<String> {
    if let Expr::Lit(value) = expr
        && let Lit::Str(text) = &value.lit
    {
        return Some(text.value());
    }
    if let Expr::Macro(value) = expr
        && value.mac.path.is_ident("concat")
    {
        let parts = Punctuated::<Expr, Token![,]>::parse_terminated
            .parse2(value.mac.tokens.clone())
            .ok()?;
        return parts
            .iter()
            .map(literal)
            .collect::<Option<Vec<_>>>()
            .map(|v| v.concat());
    }
    None
}
fn cfg(meta: &Meta, bit: u8) -> Result<bool> {
    match meta {
        Meta::Path(path) => Ok(if path.is_ident("unix") {
            bit & 3 != 0
        } else if path.is_ident("windows") {
            bit == 4
        } else {
            path.is_ident("test") || path.is_ident("debug_assertions")
        }),
        Meta::NameValue(value) => {
            let text = literal(&value.value).ok_or("Unsupported cfg value in maintained tests")?;
            let name = value
                .path
                .get_ident()
                .ok_or("Unsupported cfg path")?
                .to_string();
            Ok(match name.as_str() {
                "feature" => {
                    ["native-product", "native-browser", "test-support"].contains(&text.as_str())
                }
                "target_os" => {
                    text == match bit {
                        1 => "linux",
                        2 => "macos",
                        _ => "windows",
                    }
                }
                "target_family" => text == if bit == 4 { "windows" } else { "unix" },
                "target_env" => {
                    text == if bit == 4 {
                        "msvc"
                    } else if bit == 1 {
                        "gnu"
                    } else {
                        ""
                    }
                }
                "target_vendor" => {
                    text == if bit == 2 {
                        "apple"
                    } else if bit == 4 {
                        "pc"
                    } else {
                        "unknown"
                    }
                }
                "target_pointer_width" => text == "64",
                "target_endian" => text == "little",
                "target_arch" => text == std::env::consts::ARCH,
                "target_has_atomic" => ["8", "16", "32", "64", "ptr"].contains(&text.as_str()),
                "target_feature" | "target_abi" => false,
                _ => return fail(format!("Unreviewed test cfg key: {name}")),
            })
        }
        Meta::List(list) => {
            let args =
                Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())?;
            let values = args
                .iter()
                .map(|m| cfg(m, bit))
                .collect::<Result<Vec<_>>>()?;
            if list.path.is_ident("all") {
                Ok(values.iter().all(|v| *v))
            } else if list.path.is_ident("any") {
                Ok(values.iter().any(|v| *v))
            } else if list.path.is_ident("not") && values.len() == 1 {
                Ok(!values[0])
            } else {
                fail("Unreviewed test cfg expression")
            }
        }
    }
}
fn attributes(attrs: &[Attribute], bit: u8) -> Result<(bool, bool)> {
    fn apply(meta: &Meta, bit: u8, enabled: &mut bool, ignored: &mut bool) -> Result<()> {
        if let Meta::List(list) = meta {
            if list.path.is_ident("cfg") {
                *enabled &= cfg(&syn::parse2(list.tokens.clone())?, bit)?;
            } else if list.path.is_ident("cfg_attr") {
                let args =
                    Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())?;
                let mut args = args.iter();
                if cfg(args.next().ok_or("Empty cfg_attr")?, bit)? {
                    for attr in args {
                        apply(attr, bit, enabled, ignored)?;
                    }
                }
            }
        }
        if meta.path().is_ident("ignore") {
            *ignored = true;
        }
        Ok(())
    }
    let (mut enabled, mut ignored) = (true, false);
    for attr in attrs {
        apply(&attr.meta, bit, &mut enabled, &mut ignored)?;
    }
    Ok((enabled, ignored))
}
fn masks(attrs: &[Attribute]) -> Result<(u8, u8)> {
    let (mut enabled, mut ignored) = (0, 0);
    for bit in [1, 2, 4] {
        let (active, skip) = attributes(attrs, bit)?;
        if active {
            enabled |= bit;
            if skip {
                ignored |= bit;
            }
        }
    }
    Ok((enabled, ignored))
}
fn source_doc(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attribute| {
        if let Meta::NameValue(value) = &attribute.meta
            && value.path.is_ident("doc")
            && let Some(text) = literal(&value.value)
        {
            let text = text.trim();
            if text.starts_with("tests/") && text.contains("::") {
                return Some(text.to_owned());
            }
        }
        None
    })
}
struct Single {
    name: syn::Ident,
    id: String,
}
impl Parse for Single {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let expr = input.parse::<Expr>()?;
        let id = literal(&expr).ok_or_else(|| input.error("Source test ID must be a literal"))?;
        input.parse::<Token![,]>()?;
        input.parse::<syn::Block>()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        Ok(Self { name, id })
    }
}
struct Matrix {
    rows: Vec<Single>,
}
impl Parse for Matrix {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let prefix = input.parse::<LitStr>()?.value();
        input.parse::<Token![,]>()?;
        input.parse::<syn::Ident>()?;
        input.parse::<Token![,]>()?;
        let contents;
        syn::bracketed!(contents in input);
        let mut rows = Vec::new();
        while !contents.is_empty() {
            let name = contents.parse()?;
            contents.parse::<Token![=>]>()?;
            let args;
            syn::parenthesized!(args in contents);
            let parameter = args.parse::<LitStr>()?.value();
            args.parse::<Token![,]>()?;
            args.parse::<Expr>()?;
            if args.peek(Token![,]) {
                args.parse::<Token![,]>()?;
            }
            if !args.is_empty() {
                return Err(args.error("Unexpected source matrix argument"));
            }
            rows.push(Single {
                name,
                id: format!("{prefix}[{parameter}]"),
            });
            if contents.peek(Token![,]) {
                contents.parse::<Token![,]>()?;
            }
        }
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        Ok(Self { rows })
    }
}
impl Scanner<'_> {
    fn file(&mut self, path: &Path) -> Result<Rc<syn::File>> {
        if let Some(file) = self.files.get(path) {
            return Ok(file.clone());
        }
        if !path.canonicalize()?.starts_with(self.root.canonicalize()?)
            || fs::symlink_metadata(path)?.file_type().is_symlink()
        {
            return fail("Test module leaves the repository");
        }
        let parsed = Rc::new(syn::parse_file(&fs::read_to_string(path)?)?);
        self.files.insert(path.into(), parsed.clone());
        Ok(parsed)
    }
    fn add(
        &mut self,
        location: Location<'_>,
        name: String,
        mask: u8,
        ignored: u8,
        source: Option<String>,
    ) -> Result<()> {
        let Location {
            crate_name,
            target,
            prefix,
            path: file,
        } = location;
        let mut names = prefix.to_vec();
        names.push(name);
        let name = names.join("::");
        let key = (crate_name.into(), target.into(), name);
        let declaration = Declaration {
            file: file
                .strip_prefix(self.root)?
                .to_string_lossy()
                .replace('\\', "/"),
            mask,
            ignored: ignored & mask,
            source,
        };
        if let Some(old) = self.declarations.get_mut(&key) {
            if old.mask & mask != 0 || old.source != declaration.source {
                return fail("Duplicate applicable Rust test declaration");
            }
            old.mask |= mask;
            old.ignored |= declaration.ignored;
        } else {
            self.declarations.insert(key, declaration);
        }
        Ok(())
    }
    fn items(&mut self, walk: Walk<'_>, items: &[Item]) -> Result<()> {
        let Walk {
            location,
            directory,
            mask: parent,
            depth,
        } = walk;
        let Location {
            crate_name,
            target,
            prefix,
            path,
        } = location;
        if depth > 64 {
            return fail("Test module graph is recursive or too deep");
        }
        for item in items {
            match item {
                Item::Fn(function) if function.attrs.iter().any(|a| a.path().is_ident("test")) => {
                    let (mask, ignored) = masks(&function.attrs)?;
                    self.add(
                        location,
                        function.sig.ident.to_string(),
                        parent & mask,
                        ignored,
                        source_doc(&function.attrs),
                    )?;
                }
                Item::Macro(item)
                    if item.mac.path.is_ident("source_test")
                        || item.mac.path.is_ident("original")
                        || item.mac.path.is_ident("source_cases") =>
                {
                    let (mask, ignored) = masks(&item.attrs)?;
                    let rows = if item.mac.path.is_ident("source_cases") {
                        syn::parse2::<Matrix>(item.mac.tokens.clone())?.rows
                    } else {
                        vec![syn::parse2::<Single>(item.mac.tokens.clone())?]
                    };
                    for row in rows {
                        self.add(
                            location,
                            row.name.to_string(),
                            parent & mask,
                            ignored,
                            Some(row.id),
                        )?;
                    }
                }
                Item::Mod(module) => {
                    let (mask, _) = masks(&module.attrs)?;
                    let mask = parent & mask;
                    let mut nested = prefix.to_vec();
                    nested.push(module.ident.to_string());
                    if let Some((_, items)) = &module.content {
                        self.items(
                            Walk {
                                location: Location {
                                    crate_name,
                                    target,
                                    path,
                                    prefix: &nested,
                                },
                                directory: &directory.join(module.ident.to_string()),
                                mask,
                                depth: depth + 1,
                            },
                            items,
                        )?;
                    } else {
                        let selected = module
                            .attrs
                            .iter()
                            .find(|a| a.path().is_ident("path"))
                            .map(|attr| {
                                if let Meta::NameValue(value) = &attr.meta {
                                    literal(&value.value).ok_or("Module path is not literal")
                                } else {
                                    Err("Module path is malformed")
                                }
                            })
                            .transpose()?;
                        let child = if let Some(selected) = selected {
                            path.parent().ok_or("Module parent missing")?.join(selected)
                        } else {
                            let direct = directory.join(format!("{}.rs", module.ident));
                            if direct.exists() {
                                direct
                            } else {
                                directory.join(module.ident.to_string()).join("mod.rs")
                            }
                        };
                        if mask == 0 {
                            continue;
                        }
                        let file = self.file(&child)?;
                        let (file_mask, _) = masks(&file.attrs)?;
                        let directory = if child.file_name().is_some_and(|n| n == "mod.rs") {
                            child.parent().ok_or("Module parent missing")?.to_path_buf()
                        } else {
                            child.with_extension("")
                        };
                        self.items(
                            Walk {
                                location: Location {
                                    crate_name,
                                    target,
                                    path: &child,
                                    prefix: &nested,
                                },
                                directory: &directory,
                                mask: mask & file_mask,
                                depth: depth + 1,
                            },
                            &file.items,
                        )?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn target(&mut self, crate_name: &str, target: &str, path: &Path) -> Result<()> {
        let file = self.file(path)?;
        let (mask, _) = masks(&file.attrs)?;
        self.items(
            Walk {
                location: Location {
                    crate_name,
                    target,
                    path,
                    prefix: &[],
                },
                directory: path.parent().ok_or("Target parent missing")?,
                mask,
                depth: 0,
            },
            &file.items,
        )
    }
}

pub fn validate(root: &Path) -> Result<Inventory> {
    let baseline = decode(&fs::read(root.join("docs/rust-test-baseline.json"))?)?;
    let maps = MAPS
        .iter()
        .map(|name| decode(&fs::read(root.join("docs").join(name))?))
        .collect::<Result<Vec<_>>>()?;
    validate_documents(root, &baseline, &maps)
}
/// Public pure-document input allows the existing gate cases to mutate the real
/// inventory without changing files or adding a parallel count-only test suite.
pub fn validate_documents(root: &Path, baseline: &Value, maps: &[Value]) -> Result<Inventory> {
    let (source_ids, native_names) = frozen(baseline)?;
    if maps.len() != MAPS.len() {
        return fail("Exactly12 maintained source maps are required");
    }
    let mut scanner = Scanner {
        root,
        files: BTreeMap::new(),
        declarations: BTreeMap::new(),
    };
    scanner.target("engine-rust", "lib", &root.join("engine-rust/src/lib.rs"))?;
    for entry in fs::read_dir(root.join("engine-rust/tests"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "rs") {
            scanner.target(
                "engine-rust",
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Test target is not text")?,
                &path,
            )?;
        }
    }
    for target in ["source_ci", "source_acceptance"] {
        scanner.target(
            "tools/xtask",
            target,
            &root.join(format!("tools/xtask/tests/{target}.rs")),
        )?;
    }
    let mut inventory = Inventory {
        targets: BTreeMap::new(),
    };
    let mut seen = BTreeSet::new();
    let mut global_names = BTreeSet::new();
    for map in maps {
        if map["native_only"] != true || map["baseline_source_commit"] != SOURCE_COMMIT {
            return fail("Source map is not native or uses a different baseline");
        }
        let rows = map["cases"].as_array().ok_or("Source map cases missing")?;
        let count = map
            .get("count")
            .or_else(|| map.get("source_count"))
            .and_then(Value::as_u64)
            .ok_or("Source map count missing")?;
        if count as usize != rows.len() || rows.is_empty() {
            return fail("Source map declared count differs");
        }
        if map
            .get("implemented_count")
            .is_some_and(|n| n.as_u64() != Some(count))
        {
            return fail("Source map has pending native case declarations");
        }
        let mut kinds = BTreeMap::<&str, usize>::new();
        for row in rows {
            let id = row
                .get("python_id")
                .or_else(|| row.get("source_id"))
                .and_then(Value::as_str)
                .ok_or("Source case identity missing")?;
            if !source_ids.contains(id) || !seen.insert(id.to_owned()) {
                return fail(format!(
                    "Missing, duplicate or unknown source identity: {id}"
                ));
            }
            let kind = if id
                .split_once("::")
                .ok_or("Source identity is unqualified")?
                .1
                .contains('[')
            {
                "parameterized"
            } else {
                "standard"
            };
            if row
                .get("original_type")
                .or_else(|| row.get("case_type"))
                .and_then(Value::as_str)
                != Some(kind)
            {
                return fail(format!("Original test kind differs: {id}"));
            }
            *kinds.entry(kind).or_default() += 1;
            let name = row["rust_test"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Source ID has no actual Rust test name")?;
            let file = row["rust_file"]
                .as_str()
                .ok_or("Source ID has no Rust test declaration")?;
            safe_file(root, file)?;
            let (crate_name, target) = source_target(file)?;
            if let Some(mapped) = row.get("crate").and_then(Value::as_str)
                && crate_key(mapped)? != crate_name
            {
                return fail("Mapped crate differs from actual test file");
            }
            if row
                .get("target")
                .and_then(Value::as_str)
                .is_some_and(|s| s != target)
            {
                return fail("Mapped target differs from actual test file");
            }
            let declaration = scanner
                .declarations
                .get(&(crate_name.clone(), target.clone(), name.into()))
                .ok_or_else(|| {
                    std::io::Error::other(format!(
                        "Mapped test is not declared: {crate_name}/{target}/{name}"
                    ))
                })?;
            let mask = source_mask(id);
            if declaration.file != file
                || declaration.mask != mask
                || declaration.ignored != 0
                || declaration
                    .source
                    .as_deref()
                    .is_some_and(|source| source != id)
            {
                return fail(format!(
                    "Source declaration identity, kind or platform differs: {id}"
                ));
            }
            if let Some(platforms) = row.get("native_platforms") {
                let platforms = strings(platforms)?;
                let mut declared = 0;
                for platform in platforms {
                    let bit = platform_bit(&platform)?;
                    if declared & bit != 0 {
                        return fail("Duplicate mapped platform");
                    }
                    declared |= bit;
                }
                if declared != mask {
                    return fail(format!(
                        "Mapped platform differs from original applicability: {id}"
                    ));
                }
            }
            if !global_names.insert(name.to_owned()) {
                return fail("Ambiguous source name across crates/targets");
            }
            if inventory
                .targets
                .entry((crate_name, target))
                .or_default()
                .insert(name.into(), Case { mask })
                .is_some()
            {
                return fail("Duplicate Rust test name within crate/target");
            }
        }
        if let Some(header) = map.get("case_types") {
            for (kind, count) in kinds {
                if header[kind].as_u64() != Some(count as u64) {
                    return fail("Source map test-kind summary differs");
                }
            }
        }
    }
    if seen != source_ids {
        return fail("Source maps do not contain the exact frozen3418 identities");
    }
    let mut native_seen = BTreeSet::new();
    for ((crate_name, target, name), declaration) in &scanner.declarations {
        if crate_name == "engine-rust" && !target.starts_with("source_") {
            if !native_names.contains(name) || !native_seen.insert(name.clone()) {
                return fail(format!(
                    "Native294 test name is missing, extra or duplicated: {name}"
                ));
            }
            if declaration.mask != native_mask(name)
                || declaration.ignored != if name == LIVE { ALL } else { 0 }
            {
                return fail(format!(
                    "Native294 platform/ignore applicability differs: {name}"
                ));
            }
            if !global_names.insert(name.clone()) {
                return fail("Native/source report names overlap");
            }
            inventory
                .targets
                .entry((crate_name.clone(), target.clone()))
                .or_default()
                .insert(
                    name.clone(),
                    Case {
                        mask: declaration.mask,
                    },
                );
        }
    }
    if native_seen != native_names {
        return fail("An original native294 test declaration was removed");
    }
    // An unmapped source declaration is as serious as a missing mapped one.
    for (crate_name, target, name) in scanner.declarations.keys() {
        if target.starts_with("source_")
            && !inventory
                .targets
                .get(&(crate_name.clone(), target.clone()))
                .is_some_and(|rows| rows.contains_key(name))
        {
            return fail(format!(
                "Unmapped extra source test declaration: {crate_name}/{target}/{name}"
            ));
        }
    }
    Ok(inventory)
}

/// Every nonempty sequential harness block must finish; every expected name
/// must appear once with outcome ok. Ignored, filtered, duplicate and extra
/// names fail even when Cargo itself exited successfully.
fn checked_output(
    expected: &BTreeSet<String>,
    text: &str,
    exit: i32,
    filtered: usize,
    single: bool,
) -> Result<usize> {
    if exit != 0 || expected.is_empty() {
        return fail("Test execution failed or expected inventory is empty");
    }
    let summary = regex::Regex::new(&format!(
        r"^test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; {filtered} filtered out; finished in [0-9.]+s$",
    ))?;
    let mut active: Option<(usize, usize)> = None;
    let mut passed = BTreeSet::new();
    let mut complete = 0;
    for line in text.lines() {
        if let Some(count) = line
            .strip_prefix("running ")
            .and_then(|s| s.strip_suffix(" tests"))
            .or_else(|| {
                line.strip_prefix("running ")
                    .and_then(|s| s.strip_suffix(" test"))
            })
        {
            let count = count.parse::<usize>()?;
            if count == 0 || active.replace((count, 0)).is_some() {
                return fail("Empty, nested or incomplete test execution block");
            }
        } else if let Some((name, outcome)) = line
            .strip_prefix("test ")
            .and_then(|s| s.rsplit_once(" ... "))
        {
            let Some((_, count)) = active.as_mut() else {
                return fail("Named outcome is outside a test execution block");
            };
            if outcome != "ok" || !expected.contains(name) || !passed.insert(name.to_owned()) {
                return fail(format!(
                    "Missing, duplicate, ignored, failed or extra test outcome: {name}"
                ));
            }
            *count += 1;
        } else if line.starts_with("test result:") {
            let (announced, observed) = active
                .take()
                .ok_or("Test verdict has no started execution")?;
            let reported = summary
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|n| n.as_str().parse::<usize>())
                .transpose()?
                .ok_or("Test summary is failed, skipped, filtered or malformed")?;
            if announced != observed || observed != reported {
                return fail("Test summary/count differs from actual named outcomes");
            }
            complete += 1;
        }
    }
    if active.is_some() || complete == 0 || (single && complete != 1) || &passed != expected {
        return fail("Executed test IDs do not equal the maintained applicable inventory");
    }
    Ok(passed.len())
}
pub fn validate_expected_output(
    expected: &BTreeSet<String>,
    text: &str,
    exit: i32,
) -> Result<usize> {
    checked_output(expected, text, exit, 0, false)
}
/// Only an explicit canonical selection may report filtered tests. The skipped
/// count must equal the remaining applicable names in that same target.
pub fn validate_selected_output(
    available: &BTreeSet<String>,
    selected: &BTreeSet<String>,
    text: &str,
    exit: i32,
) -> Result<usize> {
    if selected.is_empty() || !selected.is_subset(available) {
        return fail("Explicit selection is empty or contains an unknown test identity");
    }
    checked_output(selected, text, exit, available.len() - selected.len(), true)
}
pub fn validate_target_output(
    root: &Path,
    crate_name: &str,
    target: &str,
    platform: &str,
    text: &str,
    exit: i32,
) -> Result<usize> {
    validate_expected_output(
        &validate(root)?.expected(crate_name, target, platform)?,
        text,
        exit,
    )
}
pub fn validate_native_output(root: &Path, text: &str) -> Result<usize> {
    validate_expected_output(
        &validate(root)?.native_expected(std::env::consts::OS)?,
        text,
        0,
    )
}
