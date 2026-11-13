use super::*;
use std::{collections::BTreeMap, fs, path::PathBuf};
struct Bundle {
    dir: common::TempDirectory,
    root: PathBuf,
    manifest: Value,
}
impl Bundle {
    fn new() -> Self {
        let dir = fresh();
        let root = dir.path().join("portable/browsers");
        fs::create_dir_all(&root).expect("browser contract");
        let mut manifest = json!({});
        for name in ["playwright", "patchright", "camoufox", "cloak"] {
            let path = root.join(name).join("browser.exe");
            fs::create_dir_all(path.parent().expect("browser contract")).expect("browser contract");
            fs::write(&path, name.as_bytes()).expect("browser contract");
            manifest[name] = json!({"executable":format!("{name}/browser.exe")});
        }
        fs::create_dir_all(root.join("camoufox/addons/UBO")).expect("browser contract");
        fs::write(root.join("camoufox/addons/UBO/manifest.json"), b"{}").expect("browser contract");
        manifest["camoufox"]["addons"] = json!(["camoufox/addons/UBO"]);
        manifest["camoufox"]["version"] = json!("152.0.4");
        manifest["cloak"]["version"] = json!("146.0.7680.177.5");
        let b = Self {
            dir,
            root,
            manifest,
        };
        b.save();
        b
    }
    fn save(&self) {
        fs::write(self.root.join("manifest.json"), self.manifest.to_string())
            .expect("browser contract")
    }
    fn select(
        &self,
        name: &str,
        environment: BTreeMap<String, String>,
    ) -> std::io::Result<browser::bundled::Resources> {
        browser::bundled::select(
            Some(&self.root),
            name,
            &self.dir.path().join("writable-data"),
            &environment,
            &["UBO".into()],
        )
    }
}
source_test!(
    test_chromium_backends_select_distinct_bundled_executables,
    "tests/test_bundled_engines.py::test_chromium_backends_select_distinct_bundled_executables",
    {
        let b = Bundle::new();
        let a = b
            .select("playwright", BTreeMap::new())
            .expect("browser contract");
        let p = b
            .select("patchright", BTreeMap::new())
            .expect("browser contract");
        assert_ne!(a.options["executable_path"], p.options["executable_path"]);
        assert_eq!(
            a.options["executable_path"],
            json!(b.root.join("playwright/browser.exe"))
        );
        assert_eq!(
            p.options["executable_path"],
            json!(b.root.join("patchright/browser.exe"))
        );
    }
);
source_test!(
    test_explicit_chromium_cache_override_wins_playwright,
    "tests/test_bundled_engines.py::test_explicit_chromium_cache_override_wins[playwright]",
    {
        let b = Bundle::new();
        let r = b
            .select(
                "playwright",
                BTreeMap::from([("PLAYWRIGHT_BROWSERS_PATH".into(), "/explicit/cache".into())]),
            )
            .expect("browser contract");
        assert!(r.options.is_empty());
    }
);
source_test!(
    test_explicit_chromium_cache_override_wins_patchright,
    "tests/test_bundled_engines.py::test_explicit_chromium_cache_override_wins[patchright]",
    {
        let b = Bundle::new();
        let r = b
            .select(
                "patchright",
                BTreeMap::from([("PLAYWRIGHT_BROWSERS_PATH".into(), "/explicit/cache".into())]),
            )
            .expect("browser contract");
        assert!(r.options.is_empty());
    }
);
source_test!(
    test_camoufox_uses_packaged_resources_and_default_addons,
    "tests/test_bundled_engines.py::test_camoufox_uses_packaged_resources_and_default_addons",
    {
        let b = Bundle::new();
        let r = b
            .select("camoufox", BTreeMap::new())
            .expect("browser contract");
        assert_eq!(
            r.options["executable_path"],
            json!(b.root.join("camoufox/browser.exe"))
        );
        assert_eq!(r.options["ff_version"], "152");
        assert_eq!(
            r.options["addons"],
            json!([b.root.join("camoufox/addons/UBO")])
        );
        assert_eq!(r.options["exclude_addons"], json!(["UBO"]));
    }
);
source_test!(
    test_internal_cloak_binary_is_explicit_and_user_override_wins,
    "tests/test_bundled_engines.py::test_internal_cloak_binary_is_explicit_and_user_override_wins",
    {
        let b = Bundle::new();
        let r = b
            .select("cloak", BTreeMap::new())
            .expect("browser contract");
        assert_eq!(
            r.options,
            json!({"browser_version":"146.0.7680.177.5"})
                .as_object()
                .expect("browser contract")
                .clone()
        );
        assert_eq!(
            r.environment["CLOAKBROWSER_BINARY_PATH"],
            b.root
                .join("cloak/browser.exe")
                .to_str()
                .expect("browser contract")
        );
        assert_eq!(
            b.select("cloak", r.environment.clone())
                .expect("browser contract")
                .options,
            r.options
        );
        let env = BTreeMap::from([("CLOAKBROWSER_BINARY_PATH".into(), "/user/chrome".into())]);
        let r = b.select("cloak", env.clone()).expect("browser contract");
        assert!(r.options.is_empty());
        assert!(r.environment.is_empty());
        assert_eq!(env["CLOAKBROWSER_BINARY_PATH"], "/user/chrome");
    }
);
source_test!(
    test_distribution_cloak_has_no_bundled_binary_or_override,
    "tests/test_bundled_engines.py::test_distribution_cloak_has_no_bundled_binary_or_override",
    {
        let mut b = Bundle::new();
        b.manifest
            .as_object_mut()
            .expect("browser contract")
            .remove("cloak");
        b.save();
        let r = b
            .select("cloak", BTreeMap::new())
            .expect("browser contract");
        assert!(r.options.is_empty());
        assert!(!r.environment.contains_key("CLOAKBROWSER_BINARY_PATH"));
    }
);
source_test!(
    test_geoip_is_seeded_into_writable_data_without_overwriting_refreshes,
    "tests/test_bundled_engines.py::test_geoip_is_seeded_into_writable_data_without_overwriting_refreshes",
    {
        let mut b = Bundle::new();
        let source = b.root.join("cloak-data/geoip/GeoLite2-City.mmdb");
        fs::create_dir_all(source.parent().expect("browser contract")).expect("browser contract");
        fs::write(&source, b"fixture-database").expect("browser contract");
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .expect("browser contract");
        file.set_times(
            fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            ),
        )
        .expect("browser contract");
        drop(file);
        b.manifest
            .as_object_mut()
            .expect("browser contract")
            .remove("cloak");
        b.manifest["_geoip"] = json!({"cloak":"cloak-data"});
        b.save();
        let r = b
            .select("cloak", BTreeMap::new())
            .expect("browser contract");
        assert!(r.options.is_empty());
        let cached = b
            .dir
            .path()
            .join("writable-data/browser-cache/cloak/geoip/GeoLite2-City.mmdb");
        assert_eq!(
            fs::read(&cached).expect("browser contract"),
            fs::read(&source).expect("browser contract")
        );
        assert_eq!(
            fs::metadata(&cached)
                .expect("browser contract")
                .modified()
                .expect("browser contract"),
            fs::metadata(&source)
                .expect("browser contract")
                .modified()
                .expect("browser contract")
        );
        assert_eq!(
            r.environment["CLOAKBROWSER_CACHE_DIR"],
            cached
                .parent()
                .expect("browser contract")
                .parent()
                .expect("browser contract")
                .to_str()
                .expect("browser contract")
        );
        fs::write(&cached, b"vendor-refreshed-database").expect("browser contract");
        b.select("cloak", BTreeMap::new())
            .expect("browser contract");
        assert_eq!(
            fs::read(cached).expect("browser contract"),
            b"vendor-refreshed-database"
        );
    }
);
source_test!(
    test_broken_bundle_refuses_fallback_to_a_personal_cache_missing,
    "tests/test_bundled_engines.py::test_broken_bundle_refuses_fallback_to_a_personal_cache[missing]",
    {
        let b = Bundle::new();
        fs::remove_file(b.root.join("patchright/browser.exe")).expect("browser contract");
        assert!(b.select("patchright", BTreeMap::new()).is_err());
    }
);
source_test!(
    test_broken_bundle_refuses_fallback_to_a_personal_cache_escape,
    "tests/test_bundled_engines.py::test_broken_bundle_refuses_fallback_to_a_personal_cache[escape]",
    {
        let mut b = Bundle::new();
        b.manifest["patchright"]["executable"] = json!("../browser.exe");
        b.save();
        assert!(b.select("patchright", BTreeMap::new()).is_err());
    }
);
source_test!(
    test_broken_bundle_refuses_fallback_to_a_personal_cache_missing_addon,
    "tests/test_bundled_engines.py::test_broken_bundle_refuses_fallback_to_a_personal_cache[missing-addon]",
    {
        let b = Bundle::new();
        fs::remove_file(b.root.join("camoufox/addons/UBO/manifest.json"))
            .expect("browser contract");
        assert!(b.select("camoufox", BTreeMap::new()).is_err());
    }
);
