//! Native inventory, exact occurrence coverage and owned CI temporary roots.
use super::{Result, fail};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use syn::visit::{self, Visit};
pub fn symbols(bytes: &[u8]) -> Result<Vec<String>> {
    struct Symbols {
        scope: Vec<String>,
        out: Vec<String>,
    }
    impl<'a> Visit<'a> for Symbols {
        fn visit_item_fn(&mut self, item: &'a syn::ItemFn) {
            let name = item.sig.ident.to_string();
            let mut qualified = self.scope.clone();
            qualified.push(name.clone());
            self.out.push(qualified.join("."));
            self.scope.push(name);
            visit::visit_block(self, &item.block);
            self.scope.pop();
        }
        fn visit_item_mod(&mut self, item: &'a syn::ItemMod) {
            self.scope.push(item.ident.to_string());
            visit::visit_item_mod(self, item);
            self.scope.pop();
        }
        fn visit_item_impl(&mut self, item: &'a syn::ItemImpl) {
            let name = if let syn::Type::Path(path) = item.self_ty.as_ref() {
                path.path
                    .segments
                    .iter()
                    .map(|v| v.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
            } else {
                "impl".into()
            };
            self.scope.push(name);
            visit::visit_item_impl(self, item);
            self.scope.pop();
        }
        fn visit_impl_item_fn(&mut self, item: &'a syn::ImplItemFn) {
            let name = item.sig.ident.to_string();
            let mut qualified = self.scope.clone();
            qualified.push(name.clone());
            self.out.push(qualified.join("."));
            self.scope.push(name);
            visit::visit_block(self, &item.block);
            self.scope.pop();
        }
    }
    let parsed = syn::parse_file(std::str::from_utf8(bytes)?)?;
    let mut visitor = Symbols {
        scope: Vec::new(),
        out: Vec::new(),
    };
    visitor.visit_file(&parsed);
    Ok(visitor.out)
}
pub fn describe(path: &str, bytes: &[u8]) -> Result<Value> {
    Ok(json!({"path":path,"sha256":super::sha(bytes),"symbols":symbols(bytes)?}))
}
pub fn discover(source: &str, files: &[(String, Vec<u8>)]) -> Result<Value> {
    let mut rows = Vec::new();
    let mut capabilities = Vec::new();
    for (path, bytes) in files {
        let row = describe(path, bytes)?;
        let mut seen = BTreeMap::<String, usize>::new();
        for symbol in row["symbols"]
            .as_array()
            .ok_or("Native inventory symbols missing")?
        {
            let name = symbol.as_str().ok_or("Native symbol is not text")?;
            let occurrence = seen.entry(name.into()).or_default();
            *occurrence += 1;
            let suffix = if *occurrence == 1 {
                String::new()
            } else {
                format!("#{occurrence}")
            };
            capabilities.push(json!({"id":format!("{path}::{name}{suffix}"),"target":null,"test":null,"status":"pending"}));
        }
        rows.push(row);
    }
    Ok(json!({"schema":1,"source_commit":source,"files":rows,"capabilities":capabilities}))
}
pub fn coverage(root: &Path, source: &Value, ledger: &Value) -> Result<Vec<String>> {
    let expected = source["capabilities"]
        .as_array()
        .filter(|rows| !rows.is_empty())
        .ok_or("Frozen capability inventory missing")?;
    let mut problems = Vec::new();
    let empty = Vec::new();
    let rows = ledger["capabilities"].as_array().unwrap_or(&empty);
    let mut mapped = BTreeMap::new();
    for row in rows {
        let id = row["id"].as_str().unwrap_or("");
        if mapped.insert(id, row).is_some() {
            problems.push(format!("Duplicate capability: {id}"));
        }
    }
    if ledger["source_commit"] != source["source_commit"] || ledger["files"] != source["files"] {
        problems.push("Frozen source inventory differs".into());
    }
    for expected in expected {
        let id = expected["id"]
            .as_str()
            .ok_or("Frozen capability ID missing")?;
        let Some(row) = mapped.remove(id) else {
            problems.push(format!("Missing capability: {id}"));
            continue;
        };
        if row["status"] != "verified" {
            problems.push(format!("Unverified capability: {id}"));
            continue;
        }
        for field in ["target", "test"] {
            let valid = row[field].as_str().is_some_and(|path| {
                let path = Path::new(path);
                !path.is_absolute()
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                    && root.join(path).is_file()
                    && root.join(path).canonicalize().is_ok_and(|path| {
                        root.canonicalize().is_ok_and(|root| path.starts_with(root))
                    })
            });
            if !valid {
                problems.push(format!("Unsafe capability {field}: {id}"));
            }
        }
    }
    problems.extend(mapped.keys().map(|id| format!("Unknown capability: {id}")));
    Ok(problems)
}
pub fn temporary_parent(
    root: &Path,
    environment: &BTreeMap<String, String>,
    platform: &str,
) -> Result<PathBuf> {
    const SUFFIX: &str = "/org.chromium.Chromium.123456/SingletonSocket";
    let local = root.join(".ci-work");
    if platform != "linux"
        || local.join("t000000").as_os_str().as_encoded_bytes().len() + SUFFIX.len() <= 107
    {
        return Ok(local);
    }
    let temporary = environment
        .get("RUNNER_TEMP")
        .filter(|_| {
            environment
                .get("GITHUB_ACTIONS")
                .is_some_and(|v| v == "true")
        })
        .ok_or("Chromium socket requires hosted RUNNER_TEMP")?;
    let temporary = PathBuf::from(temporary);
    if !temporary.is_absolute()
        || temporary
            .join("t000000")
            .as_os_str()
            .as_encoded_bytes()
            .len()
            + SUFFIX.len()
            > 107
    {
        return fail("Hosted RUNNER_TEMP also exceeds the Chromium socket bound");
    }
    Ok(temporary)
}
pub struct Temporary {
    path: PathBuf,
    removed: bool,
}
impl Temporary {
    pub fn retain(mut self) {
        self.removed = true;
    }
    pub fn new(
        root: &Path,
        environment: &BTreeMap<String, String>,
        platform: &str,
    ) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = temporary_parent(root, environment, platform)?;
        fs::create_dir_all(&parent)?;
        for _ in 0..1000 {
            let name = format!("t{:06x}", NEXT.fetch_add(1, Ordering::Relaxed) % 0x1000000);
            let path = parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        removed: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        fail("Owned temporary namespace unavailable")
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn finish(mut self) -> Result<()> {
        crate::acceptance::remove_owned_with(
            &self.path,
            |_, _| Ok(()),
            |diagnostic| eprintln!("{diagnostic}"),
        )?;
        self.removed = true;
        Ok(())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.removed
            && let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("Owned temporary cleanup remains unproved");
        }
    }
}
pub fn frozen_native(root: &Path) -> Result<Value> {
    let mut files = Vec::new();
    for entry in super::reference::entries(root)? {
        if entry.path.starts_with("engine-rust/src/") && entry.path.ends_with(".rs") {
            files.push((entry.path, entry.bytes));
        }
    }
    discover(super::reference::SOURCE, &files)
}
pub fn verify_legacy_inventory(document: &Value) -> Result<()> {
    let rows = document["capabilities"]
        .as_array()
        .ok_or("Characterization capability inventory missing")?;
    let ids = rows
        .iter()
        .map(|r| r["id"].as_str().ok_or("Capability ID missing"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let ordered = map.iter().collect::<BTreeMap<_, _>>();
                let mut map = serde_json::Map::new();
                for (k, v) in ordered {
                    map.insert(k.clone(), sorted(v));
                }
                Value::Object(map)
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            value => value.clone(),
        }
    }
    if ids.len() != 907
        || ids.iter().collect::<BTreeSet<_>>().len() != 907
        || document["source_commit"] != super::reference::ORIGINAL_BASELINE
        || super::sha(ids.join("\n").as_bytes())
            != "1ae989531dfa1f766d9077c75dd1e4883896c9f2201624417136b36d76aa6616"
        || super::sha(&serde_json::to_vec(&sorted(&document["files"]))?)
            != "875cf95faca683b65f84d23ba0f5ef3cf1b2445322006ff73284dbddf44e2888"
    {
        return fail("Frozen characterization inventory provenance differs");
    }
    Ok(())
}

pub fn require_profile(observed: &Value, expected: &Value) -> Result<()> {
    if !observed["updated_at"]
        .as_f64()
        .is_some_and(|v| v.is_finite() && v >= 0.0)
    {
        return fail("Replay profile timestamp is invalid");
    }
    let stable = |value: &Value| -> Result<Value> {
        let mut map = value
            .as_object()
            .ok_or("Replay profile must be object")?
            .clone();
        map.remove("updated_at");
        Ok(Value::Object(map))
    };
    crate::acceptance::require_equal(
        &stable(observed)?,
        &stable(expected)?,
        "Cross-runtime profile",
    )
}
pub fn protected(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut out = super::browser_benchmark::metadata(root)?;
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        if !path.try_exists()? {
            return Ok(());
        }
        let meta = fs::symlink_metadata(path)?;
        if meta.is_symlink() {
            out.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                format!("symlink:{}", fs::read_link(path)?.display()).into_bytes(),
            );
        } else if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(root, &entry?.path(), out)?;
            }
        } else if meta.is_file() {
            out.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path)?,
            );
        } else {
            return fail("Protected input contains a special file");
        }
        Ok(())
    }
    walk(root, &root.join("user-data"), &mut out)?;
    Ok(out)
}
