use super::release_support::*;
use super::*;
source_test!(
    c000_a_rebuild_keeps_the_data_directory,
    "tests/test_build_preserves_data.py::test_a_rebuild_keeps_the_data_directory",
    {
        let tmp = Temp::new("release-rebuild")?;
        let stage = tmp.0.join("Numera");
        fs::create_dir_all(stage.join("data/profiles"))?;
        fs::write(
            stage.join("data/profiles/real.json"),
            br#"{"name": "mine"}"#,
        )?;
        fs::write(stage.join("data/config.json"), b"{}")?;
        fs::write(stage.join("numera.exe"), b"previous build")?;
        release::assemble_stage(&stage)?;
        assert_eq!(
            fs::read(stage.join("data/profiles/real.json"))?,
            br#"{"name": "mine"}"#
        );
        assert!(stage.join("data/config.json").is_file());
        assert!(!stage.join("numera.exe").exists());
        Ok(())
    }
);
source_test!(
    c000_a_first_build_needs_no_data_directory,
    "tests/test_build_preserves_data.py::test_a_first_build_needs_no_data_directory",
    {
        let tmp = Temp::new("release-first")?;
        let stage = tmp.0.join("Numera");
        release::assemble_stage(&stage)?;
        assert!(stage.is_dir());
        Ok(())
    }
);
source_test!(
    c000_a_locked_stage_stops_the_build_instead_of_shipping_the_old_one,
    "tests/test_build_preserves_data.py::test_a_locked_stage_stops_the_build_instead_of_shipping_the_old_one",
    {
        let tmp = Temp::new("release-locked")?;
        let stage = tmp.0.join("Numera");
        fs::create_dir_all(stage.join("data/profiles"))?;
        fs::write(
            stage.join("data/profiles/real.json"),
            br#"{"name": "mine"}"#,
        )?;
        fs::create_dir(stage.join("engine"))?;
        fs::write(stage.join("engine/held.pyd"), b"in use")?;
        #[cfg(windows)]
        let _held = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(stage.join("engine/held.pyd"))?
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(stage.join("engine"), fs::Permissions::from_mode(0o500))?;
        }
        let result = release::assemble_stage(&stage);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(stage.join("engine"), fs::Permissions::from_mode(0o700))?;
        }
        assert!(
            result
                .expect_err("locked stage")
                .to_string()
                .contains("close Numera")
        );
        assert_eq!(
            fs::read(stage.join("data/profiles/real.json"))?,
            br#"{"name": "mine"}"#
        );
        Ok(())
    }
);
source_test!(
    c000_the_frozen_engine_includes_the_launch_extra,
    "tests/test_build_preserves_data.py::test_the_frozen_engine_includes_the_launch_extra",
    {
        let plan = release::build_graph(&root())?;
        assert_eq!(plan["runtime"], "rust");
        assert_eq!(plan["engine_entry"], "src/bin/engine.rs");
        assert_eq!(plan["launch"], true);
        assert_eq!(plan["api"], true);
        assert_eq!(plan["vault"], true);
        Ok(())
    }
);
source_test!(
    c000_the_freeze_entry_point_uses_an_absolute_import,
    "tests/test_build_preserves_data.py::test_the_freeze_entry_point_uses_an_absolute_import",
    {
        let plan = release::build_graph(&root())?;
        assert_eq!(plan["runtime"], "rust");
        assert_eq!(plan["engine_entry"], "src/bin/engine.rs");
        assert_eq!(plan["launch"], true);
        assert_eq!(plan["api"], true);
        assert_eq!(plan["vault"], true);
        Ok(())
    }
);
source_test!(
    c000_both_freeze_paths_use_that_entry_point,
    "tests/test_build_preserves_data.py::test_both_freeze_paths_use_that_entry_point",
    {
        let plan = release::build_graph(&root())?;
        assert_eq!(plan["runtime"], "rust");
        assert_eq!(plan["engine_entry"], "src/bin/engine.rs");
        assert_eq!(plan["launch"], true);
        assert_eq!(plan["api"], true);
        assert_eq!(plan["vault"], true);
        Ok(())
    }
);
source_test!(
    c000_the_build_downloads_the_playwright_browser,
    "tests/test_build_preserves_data.py::test_the_build_downloads_the_playwright_browser",
    { bundle("present") }
);
source_test!(
    c000_the_build_ships_the_browser_and_checks_it_is_there,
    "tests/test_build_preserves_data.py::test_the_build_ships_the_browser_and_checks_it_is_there",
    { bundle("present") }
);
source_test!(
    c000_the_engine_finds_a_bundled_browser,
    "tests/test_build_preserves_data.py::test_the_engine_finds_a_bundled_browser",
    { bundle("present") }
);
source_test!(
    c000_an_explicit_browsers_path_is_not_overridden,
    "tests/test_build_preserves_data.py::test_an_explicit_browsers_path_is_not_overridden",
    { bundle("explicit") }
);
source_test!(
    c000_no_bundle_is_not_an_error,
    "tests/test_build_preserves_data.py::test_no_bundle_is_not_an_error",
    { bundle("missing") }
);
source_test!(
    c000_an_empty_bundle_directory_is_ignored,
    "tests/test_build_preserves_data.py::test_an_empty_bundle_directory_is_ignored",
    { bundle("empty") }
);
