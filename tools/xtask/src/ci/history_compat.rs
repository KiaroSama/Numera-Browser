//! Explicit semantic adaptations for historical proofs whose old transport no
//! longer exists. Every other defect remains its exact reviewed Git snapshot.
use super::{
    Result, fail,
    history::{Case, Source},
    sha,
};
use quote::ToTokens;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
use syn::visit::Visit;

pub struct Replacement {
    pub bytes: Vec<u8>,
    pub receipt: Value,
}

/// Adapt only an old companion's call into the current fallible home resolver.
/// The selected defect file is never eligible for this caller-only bridge.
pub fn companion_replacement(
    case: &Case,
    path: &str,
    source: &Source,
    historical: &[u8],
) -> Result<Option<Replacement>> {
    if let Some(replacement) = super::history_callers::companion(case, path, source, historical)? {
        return Ok(Some(replacement));
    }
    if path != "engine-rust/src/cli/settings_commands.rs"
        || source.sha256 != "b061d6a64dd454340956b51273f09b75a123b4889f35e34cef1d99b073a34b1b"
        || case.companions.get(path) != Some(source)
    {
        return Ok(None);
    }
    let before =
        "    let path = crate::storage::data_root::expand_home(&PathBuf::from(text), home);";
    let after = "    let path = crate::storage::data_root::expand_home(&PathBuf::from(text), home)\n        .map_err(|error| RepoError::Message(error.to_string()))?;";
    caller_replacement(case, path, historical, source, &[(before, after)]).map(Some)
}

pub(super) fn caller_replacement(
    case: &Case,
    path: &str,
    bytes: &[u8],
    source: &Source,
    patches: &[(&str, &str)],
) -> Result<Replacement> {
    if !super::history::Registry::read()?.cases()?.contains(case) {
        return fail("Unreviewed historical caller bridge case");
    }
    let mut changed = std::str::from_utf8(bytes)?.to_owned();
    for (before, after) in patches {
        if changed.matches(before).count() != 1 {
            return fail("Historical caller ABI shape changed; review its bridge");
        }
        changed = changed.replacen(before, after, 1);
    }
    syn::parse_file(&changed)?;
    let changed = changed.into_bytes();
    let receipt = json!({
        "schema":1,"mode":"reviewed-caller-abi-bridge","literal_historical_execution":false,"interpreter_execution":false,
        "target":case.target,"test":case.test,"source_path":path,
        "historical_source_commit":source.commit,"historical_source_sha256":source.sha256,
        "before_sha256":sha(bytes),"after_sha256":sha(&changed),
        "bridge_patch_sha256":sha(&serde_json::to_vec(patches)?),
        "reason":"Adapt a caller to the selected snapshot's return type; selected production defect bytes remain exact Git bytes.",
        "required_negative":"exact named assertion after successful compilation","required_restored":"exact named passing test after byte-for-byte restoration"
    });
    Ok(Replacement {
        bytes: changed,
        receipt,
    })
}

/// Current HTTP code was added after the selected repository ABI. Its only
/// adaptation removes propagation from the old Option/bool return values.
pub fn additional_callers(root: &Path, case: &Case) -> Result<Vec<(String, Replacement)>> {
    let mut result = vault_callers(root, case)?;
    result.extend(atomic_callers(root, case)?);
    result.extend(super::history_callers::additional(root, case)?);
    let path = "engine-rust/src/http/profile_routes.rs";
    let sources = super::history::case_sources(case)?;
    let raw = sources
        .get("engine-rust/src/storage/repo.rs")
        .is_some_and(|source| {
            source.sha256 == "1169cbe37db745c440570f31ce14cb398448e74751383c09f07e50e50a2d229a"
        });
    let disagrees = sources
        .get("engine-rust/src/storage/repo_reconcile.rs")
        .is_some_and(|source| {
            source.sha256 == "3abbe60377f64cb60a3805ee680705cd0636db8195ca2cefd65c30e123e5c0fa"
        });
    let old_dashboard = sources.contains_key("engine-rust/src/storage/dashboard.rs");
    if !raw && !disagrees && !old_dashboard {
        return Ok(result);
    }
    // An already selected HTTP snapshot owns its original caller ABI. Leave
    // that frozen production source intact rather than applying a bridge.
    if sources.contains_key(path) {
        return Ok(result);
    }
    let old_path = if raw {
        "engine-rust/src/storage/repo.rs"
    } else if disagrees {
        "engine-rust/src/storage/repo_reconcile.rs"
    } else {
        "engine-rust/src/storage/dashboard.rs"
    };
    let source = sources
        .get(old_path)
        .ok_or("Historical caller source absent")?;
    let bytes = std::fs::read(root.join(path))?;
    let mut patches = Vec::new();
    let snapshot_rollback = match sha(&bytes).as_str() {
        "84c9dfa6892383e2d598fdfb43a246c9bd9e42688776daeb2ec648a561072552" => false,
        "6e12619ba12661ebc352c1fdab9b2ce346e0cbe085ac81c4a68c61346ee7c6ea" => {
            patches.extend(snapshot_caller_patches());
            true
        }
        _ => return fail("Current HTTP caller changed; review historical ABI bridge"),
    };
    if raw {
        patches.extend([
            (
                "let engine = ProfileRepo::raw(&state.engine.path(id, \"profiles\")?)?;",
                "let engine = ProfileRepo::raw(&state.engine.path(id, \"profiles\")?);",
            ),
            (
                "let dashboard = ProfileRepo::raw(&state.engine.path(id, \"dashboard\")?)?;",
                "let dashboard = ProfileRepo::raw(&state.engine.path(id, \"dashboard\")?);",
            ),
        ]);
    }
    if disagrees {
        patches.push((
            "                    now,\n                )?)",
            "                    now,\n                ))",
        ));
    }
    if patches.is_empty() {
        return Ok(result);
    }
    let mut replacement = caller_replacement(case, path, &bytes, source, &patches)?;
    replacement.receipt["historical_source_path"] = old_path.into();
    replacement.receipt["request_snapshot_caller_rollback"] = snapshot_rollback.into();
    replacement.receipt["reviewed_caller_base_commit"] =
        "001d8a11e120e45da932588dc10938c9182574a0".into();
    replacement.receipt["historical_sources"] = serde_json::to_value(
        sources
            .iter()
            .filter(|(path, _)| {
                [
                    "engine-rust/src/storage/repo.rs",
                    "engine-rust/src/storage/repo_reconcile.rs",
                ]
                .contains(&path.as_str())
            })
            .collect::<std::collections::BTreeMap<_, _>>(),
    )?;
    result.push((path.into(), replacement));
    Ok(result)
}

// Exact inverse of the reviewed request-local snapshot optimization. Selected
// old stores lack this later API; only these caller hunks are restored to the
// 001 native caller before adapting its Option/bool propagation operators.
fn snapshot_caller_patches() -> Vec<(&'static str, &'static str)> {
    vec![
        ("    borrow::Cow,\n", ""),
        (
            concat!(
                "    let snapshots = state.dashboard.list_for_reconciliation()?;\n",
                "    let known = snapshots\n        .iter()\n",
                "        .filter_map(|snapshot| snapshot.profile()[\"id\"].as_str())\n",
                "        .map(str::to_owned)\n        .collect::<std::collections::BTreeSet<_>>();\n",
                "    let by_path = snapshots\n        .iter()\n",
                "        .map(|snapshot| (snapshot.path(), snapshot))\n",
                "        .collect::<std::collections::BTreeMap<_, _>>();"
            ),
            concat!(
                "    let mut profiles = state.dashboard.list()?;\n",
                "    let known = profiles\n        .iter()\n",
                "        .filter_map(|profile| profile[\"id\"].as_str())\n",
                "        .map(str::to_owned)\n        .collect::<std::collections::BTreeSet<_>>();"
            ),
        ),
        (
            concat!(
                "        let path = state.engine.path(id, \"dashboard\")?;\n",
                "        let dashboard = match by_path.get(path.as_path()) {\n",
                "            Some(snapshot) => Some(snapshot.raw()?),\n",
                "            // Noncanonical spelling or a filename/body-ID mismatch must retain\n",
                "            // the original guarded lookup rather than reusing a different row.\n",
                "            None => ProfileRepo::raw(&path)?.map(Cow::Owned),\n        };"
            ),
            "        let dashboard = ProfileRepo::raw(&state.engine.path(id, \"dashboard\")?)?;",
        ),
        (
            concat!(
                "    let mut profiles = snapshots\n        .into_iter()\n",
                "        .map(|snapshot| snapshot.into_profile())\n        .collect::<Vec<_>>();\n"
            ),
            "",
        ),
    ]
}

fn atomic_callers(root: &Path, case: &Case) -> Result<Vec<(String, Replacement)>> {
    let sources = super::history::case_sources(case)?;
    let Some(source) = sources.get("engine-rust/src/storage/atomic.rs") else {
        return Ok(Vec::new());
    };
    if source.sha256 != "d02177de1a44ef62ebe74a4c2bd6bc17b588b5d2f0342d33a7eca20a4aa8c7dd" {
        return Ok(Vec::new());
    }
    let current = std::fs::read(root.join("engine-rust/src/storage/atomic.rs"))?;
    if sha(&current) != "bfadb898d01995943533722419e92613449b57a4322300c9a3ccec2b36e91cdc" {
        return fail("Current atomic error ABI changed; review historical callers");
    }
    let text = std::str::from_utf8(&current)?;
    let begin = "#[derive(Debug)]\npub struct RollbackFailed {";
    let end =
        "\n/// Snapshot bytes, never a decoded approximation, so failed changes are reversible.";
    let definition = text
        .split_once(begin)
        .and_then(|(_, rest)| {
            rest.split_once(end)
                .map(|(definition, _)| format!("{begin}{definition}"))
        })
        .ok_or("Current atomic error definition missing")?;
    let path = "engine-rust/src/vault/recovery.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "812c6ba4971a60b0dfad2a6cf1f4b79f688335828c710423d98cbe8344b377e1",
    )?;
    let imported = format!(
        "use crate::storage::locks::DataLock;\nuse historical_atomic::RollbackFailed;\n// Preserve the later caller's exact error ABI outside the frozen atomic defect.\npub(crate) mod historical_atomic {{\nuse std::{{io, path::PathBuf}};\n{definition}\n}}"
    );
    let mut recovery = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[(
            "use crate::storage::{atomic::RollbackFailed, locks::DataLock};",
            &imported,
        )],
    )?;
    recovery.receipt["historical_source_path"] = "engine-rust/src/storage/atomic.rs".into();
    recovery.receipt["included_source_path"] = "engine-rust/src/storage/atomic.rs".into();
    recovery.receipt["included_source_sha256"] = sha(&current).into();
    recovery.receipt["included_fragment_sha256"] = sha(definition.as_bytes()).into();
    let path = "engine-rust/src/storage/folders.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "b5cdee5811415d94d257b0863b7f21f1512a5fcdce89f6903287a72c5ab47d35",
    )?;
    let adapted = concat!(
        "Err(error) => {\n",
        "    let failures = batch.rollback();\n",
        "    Err(if failures.is_empty() { error } else {\n",
        "        io::Error::other(crate::vault::recovery::historical_atomic::RollbackFailed { cause: error, failures })\n",
        "    })\n",
        "},"
    );
    let mut folders = caller_replacement(
        case,
        path,
        &bytes,
        source,
        &[("Err(error) => Err(batch.fail(error)),", adapted)],
    )?;
    folders.receipt["historical_source_path"] = "engine-rust/src/storage/atomic.rs".into();
    Ok(vec![
        ("engine-rust/src/vault/recovery.rs".into(), recovery),
        (path.into(), folders),
    ])
}

pub(super) fn checked_caller(
    root: &Path,
    case: &Case,
    path: &str,
    expected: &str,
) -> Result<Vec<u8>> {
    if case.files.contains_key(path) || case.companions.contains_key(path) {
        return fail("Historical caller bridge overlaps a selected source");
    }
    let bytes = std::fs::read(root.join(path))?;
    if sha(&bytes) != expected {
        return fail("Current caller changed; review historical ABI bridge");
    }
    Ok(bytes)
}

fn vault_callers(root: &Path, case: &Case) -> Result<Vec<(String, Replacement)>> {
    let sources = super::history::case_sources(case)?;
    let Some(source) = sources.get("engine-rust/src/vault/mod.rs") else {
        return Ok(Vec::new());
    };
    // These exact snapshots all use the old tuple constructor and a private
    // durability module. The two crypto-admission variants have a private
    // tuple field, still accessible to their child policy module.
    if ![
        "ecbe0ae8c19f4ca8c15c71fab2dab37ecac024a6a05d784b5041db12ccc12dd8",
        "5fbf9a042308ce8405b66c8e2276372272157566cc2e420594e0aa9e1ae677fd",
        "71150ac122b5f5c26199ff9fcac4c042db5fcd403beb89f1a809dd0da925de3b",
    ]
    .contains(&source.sha256.as_str())
    {
        return Ok(Vec::new());
    }
    let path = "engine-rust/src/vault/policy.rs";
    let policy_hash = super::digest(&root.join(path))?;
    let constructors = match policy_hash.as_str() {
        "bf1d3cf2da498dd07ab845acfa52e92a61ca3c9242185d6158410f31b6f9ec16" => 2,
        // Reviewed SecretWriter presence/authentication guard: same tuple
        // constructor ABI, with one additional unavailable-context refusal.
        "2727287024b060c1b5d87fa15748f00d9bf33416923250daf2cfa8fe3c66d860" => 3,
        _ => return fail("Current vault policy changed; review historical constructor bridge"),
    };
    let bytes = checked_caller(root, case, path, &policy_hash)?;
    let native = std::str::from_utf8(&bytes)?;
    if native.matches("VaultError::message(").count() != constructors {
        return fail("Historical vault policy constructor call count changed");
    }
    let changed = native.replace("VaultError::message(", "VaultError(");
    let mut result = vec![(
        path.into(),
        caller_replacement(case, path, &bytes, source, &[(native, &changed)])?,
    )];
    result[0].1.receipt["historical_source_path"] = "engine-rust/src/vault/mod.rs".into();
    let path = "engine-rust/src/ownership/update_admission.rs";
    let bytes = checked_caller(
        root,
        case,
        path,
        "f144654df8ca9ae3d2339f11860672ae49f35e6c92740f9ab65b02304e1c9719",
    )?;
    let native = std::str::from_utf8(&bytes)?;
    let durability_hash = super::digest(&root.join("engine-rust/src/vault/durability.rs"))?;
    if native.matches("crate::vault::durability::").count() != 6
        || ![
            "71c871c096971d5885139937db9718133705e6f6ed3d3bbec14c3084f5aec928",
            // Same native primitive with ERROR_NOT_SUPPORTED's exact Windows
            // raw-code spelling included in its unsupported-object policy.
            "24c78df916a3285972ca1154495aa6393640bd67b096bd01563ef3d1526e32e7",
        ]
        .contains(&durability_hash.as_str())
    {
        return fail("Historical update-admission durability caller changed");
    }
    let included = concat!(
        "\n// Reviewed history-only caller bridge: use the pinned current native durability implementation.\n",
        "mod historical_vault_durability {\n",
        "    mod completion { pub use crate::vault::completion::*; }\n",
        "    mod implementation { include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/vault/durability.rs\")); }\n",
        "    pub use implementation::{sync_dir, sync_file};\n",
        "}\n"
    );
    let changed = format!(
        "{}{included}",
        native.replace(
            "crate::vault::durability::",
            "historical_vault_durability::"
        )
    );
    let mut bridge = caller_replacement(case, path, &bytes, source, &[(native, &changed)])?;
    bridge.receipt["included_source_sha256"] = durability_hash.into();
    bridge.receipt["historical_source_path"] = "engine-rust/src/vault/mod.rs".into();
    bridge.receipt["included_source_path"] = "engine-rust/src/vault/durability.rs".into();
    result.push((path.into(), bridge));
    Ok(result)
}

// Only these reviewed interpreter-era fixture versions may use their already
// ported native counterparts. Production defect sources are never selected.
const FIXTURES: &[(&str, &str, &str)] = &[
    (
        "engine-rust/tests/cli_profile_parity.rs",
        "656a8fc05ffd03f89914c586cb0fa2bc66ffde8c7f23d292269a4c702f904fe9",
        "e18176af99762fcbd710577d8e027467ff0a84815d6072f26425354b85991cd1",
    ),
    (
        "engine-rust/tests/config_parity.rs",
        "3bdc096885588ad4640650934e1118c5068c112afb045d9815be6586f4432c1a",
        "2996fa120943722f1b461d11e3d9e2dd8bbbb7b148fbc6e6bc7d541bcab39dac",
    ),
    (
        "engine-rust/tests/config_parity.rs",
        "a13add91f09aca8457fee8f40e50e2bc999808f9282c0a068a51a395afd24574",
        "2996fa120943722f1b461d11e3d9e2dd8bbbb7b148fbc6e6bc7d541bcab39dac",
    ),
    (
        "engine-rust/tests/integer_seed_parity.rs",
        "a00dffeec7f9fc93972b58cc4696958b5f4f7d6a2e37b7b90fe708a7365107cb",
        "4e6d77d3c45f96deb274a785c64ddf00d45ed648c54166677e66557e13b909bf",
    ),
    (
        "engine-rust/tests/lock_interop_parity.rs",
        "48b2c974fbf8761af26708db181fe5fae3e974216dbb6a73bc68a95a538b3b84",
        "bf78c99c9a1870a55259bb9b59a1f84a536d677a03cac3833b0e3e6a1a31257e",
    ),
    (
        "engine-rust/tests/repo_lookup_parity.rs",
        "3267e368991ce775f580b66f2068211d2835dddcf98eb37c205a2e371386d406",
        "c77735f0e037fce261a7894f23562cceb8c37837b9db51a8e3f9284e7e30c13c",
    ),
    (
        "engine-rust/tests/schedule_store_parity.rs",
        "0419aa623eb9ec63e9d442af7e683756e7d4e04436c56ef0537691c6386504a4",
        "5ab0a696bc97018a0a21d12fdb642a5f919ccf3d351a21b7c7061a42bb82eea9",
    ),
];

#[derive(Default)]
struct TestNames {
    scope: Vec<String>,
    names: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for TestNames {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.scope.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.scope.pop();
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"))
        {
            let mut scope = self.scope.clone();
            scope.push(node.sig.ident.to_string());
            self.names.insert(scope.join("::"));
        }
        syn::visit::visit_item_fn(self, node);
    }
}
fn test_names(tree: &syn::File) -> BTreeSet<String> {
    let mut names = TestNames::default();
    names.visit_file(tree);
    names.names
}

pub fn fixture_replacement(
    root: &Path,
    case: &Case,
    path: &str,
    source: &Source,
    current: &[u8],
    historical: &[u8],
) -> Result<Option<Replacement>> {
    // Other integration companions are not compiled by this exact --test
    // invocation, and remain the literal selected Git bytes.
    if path != format!("engine-rust/tests/{}.rs", case.target) {
        return Ok(None);
    }
    let Some((_, _, reviewed)) = FIXTURES
        .iter()
        .find(|(name, old, _)| *name == path && *old == source.sha256)
    else {
        return Ok(None);
    };
    let actual = sha(current);
    let reviewed = match (path, actual.as_str()) {
        // The original Linux observations and assertions remain unchanged.
        // The Windows fixture applies the source DWORD timeout bound.
        (
            "engine-rust/tests/lock_interop_parity.rs",
            "5f7fa2d2111c6d6a8a7d427b1375ccc0e6647893a46b67620376dd8ff89c3d82",
        ) => actual.as_str(),
        // Only successfully rewritten text uses native newlines; refused and
        // unchanged bytes remain exact. The helper is independently pinned.
        (
            "engine-rust/tests/schedule_store_parity.rs",
            "fa377460febc8972b275025edd0e3ec483d8ad70ebebf0bb955e5185ddfc2dea",
        ) => actual.as_str(),
        _ => *reviewed,
    };
    if actual != reviewed {
        return fail("Native historical fixture changed; review its port and assertion inventory");
    }
    let mut included_sources = Vec::new();
    if actual == "fa377460febc8972b275025edd0e3ec483d8ad70ebebf0bb955e5185ddfc2dea" {
        let helper = "engine-rust/tests/support/platform_text.rs";
        let hash = super::digest(&root.join(helper))?;
        if hash != "93ac23f8e55804d2f47979de70b499fc714f3917ccc1ca1a29eac2a193d03e06" {
            return fail("Historical native-newline fixture helper changed");
        }
        included_sources.push(json!({"path":helper,"sha256":hash}));
    }
    let old = syn::parse_file(std::str::from_utf8(historical)?)?;
    let mut native = std::str::from_utf8(current)?.to_owned();
    let mut caller_patches = Vec::new();
    if super::history::case_sources(case)?
        .get("engine-rust/src/storage/config.rs")
        .is_some_and(|source| {
            [
                "ba88fb20a4e5ac3ee40770330cde6e56c9a95fbbb94a1c51d2f0a29143652ade",
                "a803a3e5c3ce12cae8b4200e4f54ae5dec1a05e38d3cac5b99e966f360180c01",
                "45013f4fffc3dfa0d96f0e4c115ad57c5c56105564fc97cd461158c822e20612",
                "808535a3970096c53c68151cca8965f5ef261175e3eb54f85daea21ee1f4b002",
            ]
            .contains(&source.sha256.as_str())
        })
    {
        let patches: &[(&str, &str, usize)] = match path {
            "engine-rust/tests/integer_seed_parity.rs" => &[("config.read()?", "config.read()", 1)],
            "engine-rust/tests/config_parity.rs" => &[
                ("store.read()?", "store.read()", 3),
                (
                    "ConfigStore::new(root.clone()).read()?",
                    "ConfigStore::new(root.clone()).read()",
                    1,
                ),
                (
                    "store.headless_launch().map(Value::Bool)",
                    "Ok(Value::Bool(store.headless_launch()))",
                    1,
                ),
            ],
            "engine-rust/tests/lock_interop_parity.rs" => &[
                (
                    "ConfigStore::new(root).read()?",
                    "ConfigStore::new(root).read()",
                    1,
                ),
                (
                    "ConfigStore::new(root.clone()).read()?",
                    "ConfigStore::new(root.clone()).read()",
                    1,
                ),
            ],
            _ => &[],
        };
        for (before, after, count) in patches {
            if native.matches(before).count() != *count {
                return fail("Historical fixture config caller shape changed");
            }
            native = native.replace(before, after);
            caller_patches.push((*before, *after));
        }
    }
    let mut new = syn::parse_file(&native)?;
    let expected = test_names(&old);
    let available = test_names(&new);
    if expected.is_empty() || !expected.is_subset(&available) {
        return fail("Native fixture port lost an original named test");
    }
    let old_modules = old
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(module) => Some(module.ident.to_string()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut omitted = Vec::new();
    fn retain(
        items: &mut Vec<syn::Item>,
        scope: &str,
        expected: &BTreeSet<String>,
        omitted: &mut Vec<String>,
    ) {
        items.retain_mut(|item| match item {
            syn::Item::Fn(function) if function.attrs.iter().any(|a| a.path().is_ident("test")) => {
                let name = if scope.is_empty() {
                    function.sig.ident.to_string()
                } else {
                    format!("{scope}::{}", function.sig.ident)
                };
                if !expected.contains(&name) {
                    omitted.push(name);
                    false
                } else {
                    true
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, children)) = &mut module.content {
                    let name = if scope.is_empty() {
                        module.ident.to_string()
                    } else {
                        format!("{scope}::{}", module.ident)
                    };
                    retain(children, &name, expected, omitted);
                }
                true
            }
            _ => true,
        });
    }
    retain(&mut new.items, "", &expected, &mut omitted);
    new.items.retain(|item| {
        let syn::Item::Mod(module) = item else {
            return true;
        };
        if module.content.is_some() || old_modules.contains(&module.ident.to_string()) {
            return true;
        }
        let relative = module.attrs.iter().find_map(|attribute| {
            if !attribute.path().is_ident("path") {
                return None;
            }
            if let syn::Meta::NameValue(value) = &attribute.meta
                && let syn::Expr::Lit(value) = &value.value
                && let syn::Lit::Str(value) = &value.lit
            {
                return Some(value.value());
            }
            None
        });
        let Some(relative) = relative else {
            return true;
        };
        let module_path = root.join(path).parent().map(|p| p.join(relative));
        !module_path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| syn::parse_file(&s).ok())
            .is_some_and(|tree| !test_names(&tree).is_empty())
    });
    if test_names(&new) != expected {
        return fail("Historical fixture bridge changed original test count or names");
    }
    let bytes = new.into_token_stream().to_string().into_bytes();
    let receipt = json!({
        "schema":1,"mode":"reviewed-native-fixture-bridge","literal_historical_execution":false,"interpreter_execution":false,
        "reason":"Replace only the selected legacy oracle fixture with its immutable-observation/Rust-helper port; preserve every old named test and omit later-added tests.",
        "target":case.target,"test":case.test,"source_path":path,"historical_source_commit":source.commit,"historical_source_sha256":source.sha256,
        "current_before_sha256":sha(current),"ported_counterpart_sha256":reviewed,"mutated_after_sha256":sha(&bytes),
        "bridge_patch_sha256":sha(&[historical,b"\0native-fixture-port\0",&bytes].concat()),
        "caller_abi_patches":caller_patches,"included_sources":included_sources,
        "original_tests":expected,"omitted_later_tests":omitted,"oracle_transport_assertion":"Legacy interpreter process-success checks become fallible immutable JSON decoding and real Rust fixture setup; behavioral/storage assertions remain.",
        "required_negative":"exact named assertion after successful compilation","required_restored":"exact named passing test after byte-for-byte restoration"
    });
    Ok(Some(Replacement { bytes, receipt }))
}

pub fn replacement(
    case: &Case,
    path: &str,
    source: &Source,
    current: &[u8],
) -> Result<Option<Replacement>> {
    if case.target != "native_environment"
        || case.test != "metadata_discovery_uses_sdk_only_adapter_and_rejects_malformed_receipts"
    {
        return Ok(None);
    }
    if path != "engine-rust/src/native_environment.rs"
        || source.commit != "d1c443d4cdef191dcb7c4e82cf6194a2e85b2b33"
        || source.sha256 != "0fae0f236ffb9f71423a0d362a570edbe3e26c92d2b77b30377f060fa3ecaec5"
        || case.commit != source.commit
        || case.files.get(path) != Some(&source.sha256)
        || case.files.len() != 1
        || !case.companions.is_empty()
        || case.symptom != "SDK metadata wrote bytecode into runtime inputs"
    {
        return fail("Unreviewed historical metadata adaptation identity");
    }
    let current = std::str::from_utf8(current)?;
    let anchor = "        let value: Value = serde_json::from_slice(&result.stdout).map_err(io::Error::other)?;";
    if current.matches(anchor).count() != 1 {
        return fail("Current native metadata shape changed; review the semantic mutation");
    }
    let defect = concat!(
        "        // Historical native-equivalent defect: metadata caches under application inputs.\n",
        "        std::fs::write(self.app.join(\".numera-metadata-cache\"), &result.stdout)\n",
        "            .map_err(capture::Failure::from)?;\n"
    );
    let changed = current
        .replacen(anchor, &format!("{defect}{anchor}"), 1)
        .into_bytes();
    let receipt = json!({
        "schema":1,"mode":"native-equivalent-semantic-mutation",
        "literal_historical_execution":false,"interpreter_execution":false,
        "reason":"The old interpreted SDK import cache write is represented by a real native metadata cache write under the same application inputs.",
        "target":case.target,"test":case.test,"symptom":case.symptom,
        "source_path":path,"historical_source_commit":source.commit,
        "historical_source_sha256":source.sha256,
        "current_before_sha256":sha(current.as_bytes()),
        "mutated_after_sha256":sha(&changed),"patch_sha256":sha(defect.as_bytes()),
        "required_negative":"exact named assertion after successful compilation",
        "required_restored":"exact named passing test after byte-for-byte restoration"
    });
    Ok(Some(Replacement {
        bytes: changed,
        receipt,
    }))
}
