use super::*;
use ci::version;
source_test!(
    version_manifests,
    "tests/test_version_is_one_number.py::test_every_manifest_declares_the_same_version",
    {
        let found = version::declared(&root())?;
        assert_eq!(found.len(), 5);
        assert_eq!(found.values().collect::<BTreeSet<_>>().len(), 1);
        version::verify(&root())?;
        Ok(())
    }
);
source_test!(
    version_runtime,
    "tests/test_version_is_one_number.py::test_the_running_package_reports_that_version",
    {
        let found = version::declared(&root())?;
        assert_eq!(
            numera_core::cli::registry().get_version(),
            Some(found["engine-rust/Cargo.toml"].as_str())
        );
        assert_eq!(env!("CARGO_PKG_VERSION"), found["engine-rust/Cargo.toml"]);
        Ok(())
    }
);
source_test!(
    version_semantic,
    "tests/test_version_is_one_number.py::test_the_version_is_a_release_number",
    {
        let version = &version::declared(&root())?["engine-rust/Cargo.toml"];
        assert!(version::release_number(version));
        for bad in ["1.2", "1.2.3\n", "v1.2.3", "1.2.3-dev", "1.2.3.4"] {
            assert!(!version::release_number(bad));
        }
        Ok(())
    }
);
source_test!(
    version_changelog,
    "tests/test_version_is_one_number.py::test_the_changelog_has_an_entry_for_it",
    {
        let version = &version::declared(&root())?["engine-rust/Cargo.toml"];
        assert!(
            fs::read_to_string(root().join("CHANGELOG.md"))?.contains(&format!("## [{version}]"))
        );
        Ok(())
    }
);
