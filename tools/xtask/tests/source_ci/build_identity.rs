use super::release_support::*;
use super::*;
source_test!(
    c000_generated_frozen_module_and_pe_resource_carry_same_exact_identity,
    "tests/test_build_identity.py::test_generated_frozen_module_and_pe_resource_carry_same_exact_identity",
    {
        let (tmp, _, record) = build()?;
        let generated = tmp.0.join("generated");
        let returned = release::prepare_identity(&generated, record.clone(), None)?;
        let embedded: Value =
            serde_json::from_slice(&fs::read(generated.join("build-identity.json"))?)?;
        assert_eq!(embedded, record);
        assert_eq!(returned["source_sha"], "a".repeat(40));
        let resource: Value =
            serde_json::from_slice(&fs::read(generated.join("windows-version.json"))?)?;
        assert_eq!(resource["CompanyName"], "KiaroSama");
        assert_eq!(resource["filevers"], json!([0, 36, 0, 0]));
        assert_eq!(resource["PrivateBuild"], record["source_sha"]);
        Ok(())
    }
);
source_test!(
    c000_identity_refuses_wrong_source_and_invalid_hosted_generation,
    "tests/test_build_identity.py::test_identity_refuses_wrong_source_and_invalid_hosted_generation[GITHUB_SHA-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb]",
    {
        let (tmp, mut env, _) = build()?;
        env.insert(
            "GITHUB_SHA".into(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        );
        assert!(release::build_identity(&tmp.0, &"a".repeat(40), &env).is_err());
        Ok(())
    }
);
source_test!(
    c001_identity_refuses_wrong_source_and_invalid_hosted_generation,
    "tests/test_build_identity.py::test_identity_refuses_wrong_source_and_invalid_hosted_generation[GITHUB_RUN_ID-0]",
    {
        let (tmp, mut env, _) = build()?;
        env.insert("GITHUB_RUN_ID".into(), "0".into());
        assert!(release::build_identity(&tmp.0, &"a".repeat(40), &env).is_err());
        Ok(())
    }
);
source_test!(
    c002_identity_refuses_wrong_source_and_invalid_hosted_generation,
    "tests/test_build_identity.py::test_identity_refuses_wrong_source_and_invalid_hosted_generation[GITHUB_RUN_ID--1]",
    {
        let (tmp, mut env, _) = build()?;
        env.insert("GITHUB_RUN_ID".into(), "-1".into());
        assert!(release::build_identity(&tmp.0, &"a".repeat(40), &env).is_err());
        Ok(())
    }
);
source_test!(
    c003_identity_refuses_wrong_source_and_invalid_hosted_generation,
    "tests/test_build_identity.py::test_identity_refuses_wrong_source_and_invalid_hosted_generation[GITHUB_RUN_ID-42\\n]",
    {
        let (tmp, mut env, _) = build()?;
        env.insert("GITHUB_RUN_ID".into(), "42\n".into());
        assert!(release::build_identity(&tmp.0, &"a".repeat(40), &env).is_err());
        Ok(())
    }
);
source_test!(
    c000_verification_rejects_different_embedded_generation_without_signing_inputs,
    "tests/test_build_identity.py::test_verification_rejects_different_embedded_generation_without_signing_inputs",
    {
        let (tmp, _, record) = build()?;
        engines(&tmp)?;
        let mut calls = Vec::new();
        let result = release::verify_embedded_with(&tmp.0, &record, &private_env(), |path, env| {
            assert!(private_env().keys().all(|name| !env.contains_key(name)));
            calls.push(path.to_owned());
            let mut actual = record.clone();
            if calls.len() == 2 {
                actual["generation"] = 41.into();
            }
            Ok(serde_json::to_vec(&actual)?)
        });
        assert!(
            result
                .expect_err("wrong engine generation")
                .to_string()
                .contains("Embedded executable identity")
        );
        assert_eq!(calls.len(), 2);
        Ok(())
    }
);
source_test!(
    c000_fixture_generation_is_embedded_only_when_explicitly_requested,
    "tests/test_build_identity.py::test_fixture_generation_is_embedded_only_when_explicitly_requested",
    {
        let (tmp, _, record) = build()?;
        let fixture = release::prepare_identity(&tmp.0.join("fixture"), record.clone(), Some(85))?;
        assert_eq!(fixture["generation"], 85);
        assert_eq!(fixture["acceptance"], true);
        assert!(record.get("acceptance").is_none());
        assert_eq!(record["generation"], 42);
        assert!(
            release::prepare_identity(&tmp.0.join("invalid"), record, Some(86))
                .expect_err("unapproved fixture generation")
                .to_string()
                .contains("hosted fixture generation")
        );
        Ok(())
    }
);
source_test!(
    c000_publication_refuses_a_fixture_compiled_executable,
    "tests/test_build_identity.py::test_publication_refuses_a_fixture_compiled_executable",
    {
        let (tmp, env, record) = build()?;
        engines(&tmp)?;
        let result = release::verify_embedded_with(&tmp.0, &record, &env, |_, _| {
            let mut bad = record.clone();
            bad["acceptance"] = true.into();
            Ok(serde_json::to_vec(&bad)?)
        });
        assert!(
            result
                .expect_err("fixturebinary")
                .to_string()
                .contains("Embedded executable identity")
        );
        Ok(())
    }
);
source_test!(
    c000_both_freezers_embed_identity_and_windows_uses_approved_icon,
    "tests/test_build_identity.py::test_both_freezers_embed_identity_and_windows_uses_approved_icon",
    {
        let plan = release::build_graph(&root())?;
        assert_eq!(plan["runtime"], "rust");
        assert_eq!(plan["engine_entry"], "src/bin/engine.rs");
        assert!(
            root()
                .join(plan["icon"].as_str().expect("approved icon"))
                .is_file()
        );
        let source = fs::read_to_string(root().join("engine-rust/src/bin/engine.rs"))?;
        assert!(source.contains("--build-info"));
        assert!(source.contains("numera_core::native_build::identity"));
        windows_signing_fixture()?;
        Ok(())
    }
);
