use super::*;
use numera_core::browser::{alignment, trust_grades};
fn windows_seen(p: &Profile) -> Value {
    json!({"webdriver":false,"webdriverOwn":false,"hasChrome":true,"plugins":3,
        "userAgent":p.fingerprint.user_agent,"platform":"Win32","languages":["en-US"],
        "cores":16,"memory":8,"screenW":1920,"screenH":1080,"timezone":p.fingerprint.timezone,
        "toStringNative":true,"permissionsAgree":true,"canvasStable":true,"canvasEmpty":false,
        "webglVendor":"Google Inc. (NVIDIA)","webglRenderer":"ANGLE (NVIDIA, NVIDIA GeForce RTX 3050 Ti Direct3D11 vs_5_0 ps_5_0, D3D11)",
        "fontKnown":false,"fontAlien":false,"cameras":1,"microphones":1,
        "fontsRendered":["Segoe UI","Calibri","Consolas","Arial","Tahoma"]})
}
fn consistent_seen(tz: &str) -> Value {
    json!({"webdriver":false,"webdriverOwn":false,"hasChrome":true,"plugins":3,
        "userAgent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        "platform":"Win32","languages":["en-US","en"],"cores":8,"memory":8,"timezone":tz,
        "toStringNative":true,"permissionsAgree":true,"canvasStable":true,"canvasEmpty":false,
        "webglVendor":"Google Inc. (NVIDIA)","webglRenderer":"ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        "screenW":1920,"screenH":1080,"fontsRendered":[catalog()["FONTS"]["windows"][0]],"cameras":1,"microphones":1})
}
// The original tests stub _observe. This is the same pure reconciliation and
// persistence sequence used after observation by cli::align_commands::Align.
fn align(
    repo: &ProfileRepo,
    p: &Profile,
    seen: &Value,
    engine: &str,
) -> (Profile, Vec<String>, Value, Value) {
    repo.save(p.clone(), 1000.0).expect("save original");
    let before = trust_grades::grade(&p.fingerprint, seen, engine).expect("grade original");
    let (rewritten, changed) =
        alignment::rewrite(&p.fingerprint, seen).expect("reconcile observation");
    let saved = repo
        .edit(
            &p.id,
            |current| {
                alignment::patch(&mut current.fingerprint, &rewritten)?;
                Ok(None)
            },
            1001.0,
        )
        .expect("save alignment")
        .expect("profile exists");
    let mut adjusted = seen.clone();
    adjusted["userAgent"] = saved.fingerprint.user_agent.clone().into();
    let after =
        trust_grades::grade(&saved.fingerprint, &adjusted, engine).expect("grade rewritten");
    (saved, changed, before, after)
}
source_test!(
    os_inferred_from_platform,
    "tests/test_align.py::test_os_inferred_from_platform",
    {
        for (platform, fallback, expected) in [
            ("Win32", "macos", "windows"),
            ("MacIntel", "windows", "macos"),
            ("Linux x86_64", "windows", "linux"),
            ("Linux armv8l", "windows", "android"),
            ("", "linux", "linux"),
        ] {
            let fp = generate(1, Some(fallback), Some("en-US"));
            assert_eq!(
                alignment::rewrite(&fp, &json!({"platform":platform}))
                    .expect("platform reconciliation")
                    .0
                    .os,
                expected
            );
        }
    }
);
source_test!(
    chrome_pulled_from_ua,
    "tests/test_align.py::test_chrome_pulled_from_ua",
    {
        let fp = generate(1, Some("windows"), None);
        assert_eq!(
            alignment::rewrite(
                &fp,
                &json!({"userAgent":"... Chrome/127.0.0.0 Safari/537.36"})
            )
            .expect("UA reconciliation")
            .0
            .chrome_version,
            "127.0.0.0"
        );
        assert_eq!(
            alignment::rewrite(&fp, &json!({"userAgent":"no version here"}))
                .expect("UA fallback")
                .0
                .chrome_version,
            fp.chrome_version
        );
    }
);
source_test!(
    vendor_guessed_from_renderer,
    "tests/test_align.py::test_vendor_guessed_from_renderer",
    {
        let fp = generate(1, Some("windows"), None);
        for (renderer, vendor) in [
            ("ANGLE (NVIDIA, RTX 3050 Ti, D3D11)", "NVIDIA"),
            ("ANGLE (Apple, Apple M1, OpenGL 4.1)", "Apple"),
        ] {
            assert!(
                alignment::rewrite(&fp, &json!({"webglRenderer":renderer}))
                    .expect("GPU reconciliation")
                    .0
                    .webgl_vendor
                    .contains(vendor)
            );
        }
    }
);
source_test!(
    align_reconciles_mac_profile_to_windows_host,
    "tests/test_align.py::test_align_reconciles_mac_profile_to_windows_host",
    {
        let temp = common::TempDirectory::new("source-align-win").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let mut p = profile("mac-on-win", 1, "macos");
        p.engine = "cloak".into();
        let (saved, changed, before, after) = align(&repo, &p, &windows_seen(&p), "cloak");
        let fp = saved.fingerprint;
        assert_eq!(fp.os, "windows");
        assert_eq!(fp.platform, "Win32");
        assert_eq!(fp.hardware_concurrency, 16);
        assert_eq!(fp.device_memory, json!(8));
        assert!(fp.webgl_renderer.contains("RTX 3050 Ti"));
        assert!(fp.user_agent.starts_with("Mozilla/5.0 (Windows NT"));
        assert!(before["score"].as_f64().expect("score") < after["score"].as_f64().expect("score"));
        assert_eq!(after["score"], 100);
        for field in [
            "os",
            "platform",
            "userAgent",
            "cores",
            "memory",
            "webglRenderer",
            "fonts",
        ] {
            assert!(changed.iter().any(|s| s == field), "{field}: {changed:?}");
        }
    }
);
source_test!(
    aligned_profile_is_coherent,
    "tests/test_align.py::test_aligned_profile_is_coherent",
    {
        let temp = common::TempDirectory::new("source-align-coherence").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("p", 2, "macos");
        let (saved, _, _, _) = align(&repo, &p, &windows_seen(&p), "cloak");
        assert!(
            identity::validate(&saved.fingerprint)
                .expect("validate")
                .is_empty()
        );
    }
);
source_test!(
    align_is_a_noop_when_already_matching,
    "tests/test_align.py::test_align_is_a_noop_when_already_matching",
    {
        let temp = common::TempDirectory::new("source-align-noop").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("p", 3, "windows");
        let fp = &p.fingerprint;
        let seen = json!({"webdriver":false,"webdriverOwn":false,"hasChrome":true,"plugins":3,"userAgent":fp.user_agent,"platform":fp.platform,
        "languages":fp.languages,"cores":fp.hardware_concurrency,"memory":fp.device_memory,"screenW":fp.screen_width,"screenH":fp.screen_height,
        "timezone":fp.timezone,"toStringNative":true,"permissionsAgree":true,"canvasStable":true,"canvasEmpty":false,"webglVendor":fp.webgl_vendor,
        "webglRenderer":fp.webgl_renderer,"fontKnown":true,"fontAlien":false,"cameras":fp.cameras,"microphones":fp.microphones,"fontsRendered":fp.fonts});
        let (_, changed, _, after) = align(&repo, &p, &seen, "playwright");
        assert!(changed.is_empty(), "{changed:?}");
        assert_eq!(after["score"], 100);
    }
);
source_test!(
    align_moves_timezone_and_locale_to_reality,
    "tests/test_align.py::test_align_moves_timezone_and_locale_to_reality",
    {
        let temp = common::TempDirectory::new("source-align-timezone").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let mut p = profile("flow", 1, "windows");
        p.engine = "cloak".into();
        let (saved, changed, _, after) =
            align(&repo, &p, &consistent_seen("Asia/Karachi"), "cloak");
        assert_eq!(saved.fingerprint.timezone, "Asia/Karachi");
        assert_eq!(saved.fingerprint.locale, "en-PK");
        assert!(changed.iter().any(|s| s == "timezone"));
        assert_eq!(after["grade"], "A");
        assert_eq!(after["score"], 100);
        assert!(
            identity::validate(&saved.fingerprint)
                .expect("validate")
                .is_empty()
        );
    }
);
source_test!(
    align_keeps_timezone_when_no_locale_models_it,
    "tests/test_align.py::test_align_keeps_timezone_when_no_locale_models_it",
    {
        let temp = common::TempDirectory::new("source-align-unknown-zone").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("flow", 1, "windows");
        let (saved, changed, _, _) =
            align(&repo, &p, &consistent_seen("Antarctica/Troll"), "cloak");
        assert_eq!(
            json!(saved.fingerprint.timezone),
            catalog()["LOCALES"]["en-US"][0]
        );
        assert!(!changed.iter().any(|s| s == "timezone"));
    }
);
source_test!(
    crossing_os_families_resets_the_values_the_old_os_owned,
    "tests/test_align.py::test_crossing_os_families_resets_the_values_the_old_os_owned",
    {
        let temp = common::TempDirectory::new("source-align-os-reset").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let mut p = profile("mac-to-win", 3, "macos");
        p.engine = "cloak".into();
        let mac = p.fingerprint.os_version.clone();
        assert!(["13", "14", "15"].iter().any(|v| mac.starts_with(v)));
        align(&repo, &p, &windows_seen(&p), "cloak");
        let fp = ProfileRepo::new(temp.path().to_owned(), None)
            .resolve("mac-to-win", 1002.0)
            .expect("resolve saved")
            .expect("profile")
            .fingerprint;
        assert_eq!(fp.os, "windows");
        assert_ne!(fp.os_version, mac);
        assert!(
            catalog()["OS_VERSIONS"]["windows"]
                .as_array()
                .expect("versions")
                .iter()
                .any(|v| v[0] == fp.os_version)
        );
        assert!(fp.device_model.is_empty());
        assert!(fp.user_agent.contains("Windows NT") && !fp.user_agent.contains("Mac OS X"));
    }
);
source_test!(
    the_client_hints_agree_with_the_new_os,
    "tests/test_align.py::test_the_client_hints_agree_with_the_new_os",
    {
        let temp = common::TempDirectory::new("source-align-hints").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("hints", 4, "macos");
        let stale =
            identity::platform_version("macos", &p.fingerprint.os_version).expect("stale hint");
        let (saved, _, _, _) = align(&repo, &p, &windows_seen(&p), "cloak");
        let fp = &saved.fingerprint;
        let hint = identity::platform_version(&fp.os, &fp.os_version).expect("new hint");
        assert_ne!(hint, stale);
        assert!(
            catalog()["OS_VERSIONS"]["windows"]
                .as_array()
                .expect("versions")
                .iter()
                .any(|v| v[1] == hint)
        );
        let script = numera_core::identity_script::build(fp).expect("identity script");
        assert!(
            script.contains(&format!("\"platformVersion\": \"{hint}\""))
                || script.contains(&format!("'platformVersion': '{hint}'"))
        );
        assert!(script.contains("\"uaPlatform\": \"Windows\""));
    }
);
source_test!(
    touch_and_dpr_follow_the_observed_machine,
    "tests/test_align.py::test_touch_and_dpr_follow_the_observed_machine",
    {
        let temp = common::TempDirectory::new("source-align-touch").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("touch", 5, "android");
        assert_eq!(p.fingerprint.max_touch_points, 5);
        let mut seen = windows_seen(&p);
        seen["maxTouchPoints"] = json!(0);
        seen["devicePixelRatio"] = json!(1.25);
        let (saved, _, _, _) = align(&repo, &p, &seen, "cloak");
        assert_eq!(saved.fingerprint.os, "windows");
        assert_eq!(saved.fingerprint.max_touch_points, 0);
        assert_eq!(saved.fingerprint.device_pixel_ratio.as_f64(), Some(1.25));
        assert!(!saved.fingerprint.is_mobile);
    }
);
source_test!(
    an_unobserved_form_factor_falls_back_to_the_new_os,
    "tests/test_align.py::test_an_unobserved_form_factor_falls_back_to_the_new_os",
    {
        let temp =
            common::TempDirectory::new("source-align-form-fallback").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let p = profile("fallback", 6, "android");
        let (saved, _, _, _) = align(&repo, &p, &windows_seen(&p), "cloak");
        let d = catalog();
        assert!(
            d["TOUCH_POINTS"]["windows"]
                .as_array()
                .expect("touch pool")
                .contains(&json!(saved.fingerprint.max_touch_points))
        );
        assert!(
            d["PIXEL_RATIOS"]["windows"]
                .as_array()
                .expect("ratio pool")
                .contains(&json!(saved.fingerprint.device_pixel_ratio))
        );
    }
);
