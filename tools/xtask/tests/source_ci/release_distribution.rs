use super::release_support::*;
use super::*;
use ci::release_signing as signing;
source_test!(
    c000_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.ai/note.md]",
    {
        assert!(
            release::public_path(".ai/note.md", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c001_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.specify/config.json]",
    {
        assert!(
            release::public_path(".specify/config.json", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c002_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[specs/task.md]",
    {
        assert!(
            release::public_path("specs/task.md", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c003_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[secrets.md]",
    {
        assert!(
            release::public_path("secrets.md", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c004_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[docs/secrets.md]",
    {
        assert!(
            release::public_path("docs/secrets.md", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c005_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.env]",
    {
        assert!(
            release::public_path(".env", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c006_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.env.production]",
    {
        assert!(
            release::public_path(".env.production", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c007_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.git/config]",
    {
        assert!(
            release::public_path(".git/config", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c008_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[.ignoreme/data]",
    {
        assert!(
            release::public_path(".ignoreme/data", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c009_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[data/profiles.json]",
    {
        assert!(
            release::public_path("data/profiles.json", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c010_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[node_modules/package/index.js]",
    {
        assert!(
            release::public_path("node_modules/package/index.js", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c011_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[../secret]",
    {
        assert!(
            release::public_path("../secret", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c012_source_archive_rejects_private_or_generated_paths,
    "tests/test_release_distribution.py::test_source_archive_rejects_private_or_generated_paths[/absolute]",
    {
        assert!(
            release::public_path("/absolute", false)
                .expect_err("private publication path")
                .to_string()
                .contains("publication")
        );
        Ok(())
    }
);
source_test!(
    c000_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[engine/numera/cli/__init__.py]",
    { release::public_path("engine/numera/cli/__init__.py", false) }
);
source_test!(
    c001_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[engine/tests/fixtures/sample.json]",
    { release::public_path("engine/tests/fixtures/sample.json", false) }
);
source_test!(
    c002_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[dashboard/package-lock.json]",
    { release::public_path("dashboard/package-lock.json", false) }
);
source_test!(
    c003_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[.env.example]",
    { release::public_path(".env.example", false) }
);
source_test!(
    c004_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[.github/workflows/ci.yml]",
    { release::public_path(".github/workflows/ci.yml", false) }
);
source_test!(
    c005_complete_source_keeps_build_files_and_safe_fixtures,
    "tests/test_release_distribution.py::test_complete_source_keeps_build_files_and_safe_fixtures[docs/licensing.md]",
    { release::public_path("docs/licensing.md", false) }
);
source_test!(
    c000_missing_source_license_or_cloak_binary_refuses_download,
    "tests/test_release_distribution.py::test_missing_source_license_or_cloak_binary_refuses_download",
    {
        let tmp = Temp::new("release-stage")?;
        let stage = tmp.0.join("Numera Browser");
        fs::create_dir(&stage)?;
        let source = "a".repeat(40);
        assert!(
            release::validate_stage(&stage, &source)
                .expect_err("LICENSE missing")
                .to_string()
                .contains("LICENSE")
        );
        fs::write(stage.join("LICENSE"), b"license")?;
        assert!(
            release::validate_stage(&stage, &source)
                .expect_err("source missing")
                .to_string()
                .contains("source")
        );
        fs::create_dir(stage.join("sources"))?;
        fs::write(
            stage
                .join("sources")
                .join(format!("numera-source-{source}.tar.gz")),
            b"source",
        )?;
        fs::create_dir(stage.join("browsers"))?;
        fs::write(
            stage.join("browsers/manifest.json"),
            br#"{"cloak":{"executable":"cloak/chrome"}}"#,
        )?;
        assert!(
            release::validate_stage(&stage, &source)
                .expect_err("Cloak distribution")
                .to_string()
                .contains("Cloak")
        );
        Ok(())
    }
);
source_test!(
    c000_asset_hashing_retains_python310_with_public_sha256_vectors,
    "tests/test_release_distribution.py::test_asset_hashing_retains_python310_with_public_sha256_vectors[empty]",
    {
        let tmp = Temp::new("release-digest")?;
        let path = tmp.write("numera-fixture.zip", &[])?;
        assert_eq!(
            release::asset(&path)?["sha256"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        Ok(())
    }
);
source_test!(
    c001_asset_hashing_retains_python310_with_public_sha256_vectors,
    "tests/test_release_distribution.py::test_asset_hashing_retains_python310_with_public_sha256_vectors[abc]",
    {
        let tmp = Temp::new("release-digest")?;
        let path = tmp.write("numera-fixture.zip", b"abc")?;
        assert_eq!(
            release::asset(&path)?["sha256"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }
);
source_test!(
    c002_asset_hashing_retains_python310_with_public_sha256_vectors,
    "tests/test_release_distribution.py::test_asset_hashing_retains_python310_with_public_sha256_vectors[million-a]",
    {
        let tmp = Temp::new("release-digest")?;
        let path = tmp.write("numera-fixture.zip", &vec![97; 1000000])?;
        assert_eq!(
            release::asset(&path)?["sha256"],
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        Ok(())
    }
);
source_test!(
    c000_manifest_verification_rejects_tampered_or_unknown_assets,
    "tests/test_release_distribution.py::test_manifest_verification_rejects_tampered_or_unknown_assets",
    {
        let tmp = Temp::new("release-manifest")?;
        let asset = tmp.write("numera-linux-x64.tar.gz", b"original archive")?;
        let record = release::asset(&asset)?;
        let manifest = json!({"schema":1,"source_sha":"a".repeat(40),"assets":[record]});
        release::verify_distribution(&tmp.0, &manifest, &"a".repeat(40), false)?;
        fs::write(&asset, b"changed archive")?;
        assert!(
            release::verify_distribution(&tmp.0, &manifest, &"a".repeat(40), false)
                .expect_err("tampered")
                .to_string()
                .contains("checksum")
        );
        fs::write(&asset, b"original archive")?;
        tmp.write("private.log", b"diagnostic")?;
        assert!(
            release::verify_distribution(&tmp.0, &manifest, &"a".repeat(40), false)
                .expect_err("unknown asset")
                .to_string()
                .contains("unexpected")
        );
        Ok(())
    }
);
source_test!(
    c000_manifest_requires_exact_sha_and_unique_safe_names,
    "tests/test_release_distribution.py::test_manifest_requires_exact_sha_and_unique_safe_names",
    {
        let tmp = Temp::new("release-names")?;
        let asset = tmp.write("numera-linux-x64.tar.gz", b"archive")?;
        let record = release::asset(&asset)?;
        let mut manifest =
            json!({"schema":1,"source_sha":"a".repeat(40),"assets":[record.clone()]});
        assert!(
            release::verify_distribution(&tmp.0, &manifest, &"b".repeat(40), false)
                .expect_err("SHA")
                .to_string()
                .contains("SHA")
        );
        manifest["assets"]
            .as_array_mut()
            .expect("assets")
            .push(record);
        assert!(
            release::verify_distribution(&tmp.0, &manifest, &"a".repeat(40), false)
                .expect_err("duplicate")
                .to_string()
                .contains("duplicate")
        );
        manifest["assets"] = json!([{"name":"../escape.tar.gz"}]);
        assert!(
            release::verify_distribution(&tmp.0, &manifest, &"a".repeat(40), false)
                .expect_err("unsafe name")
                .to_string()
                .contains("name")
        );
        Ok(())
    }
);
source_test!(
    c000_archive_excludes_private_update_runtime_state,
    "tests/test_release_distribution.py::test_archive_excludes_private_update_runtime_state[.tar.gz]",
    { archive_runtime(".tar.gz") }
);
source_test!(
    c001_archive_excludes_private_update_runtime_state,
    "tests/test_release_distribution.py::test_archive_excludes_private_update_runtime_state[.zip]",
    { archive_runtime(".zip") }
);
source_test!(
    c000_posix_archive_keeps_executable_permissions,
    "tests/test_release_distribution.py::test_posix_archive_keeps_executable_permissions[executable]",
    { permission_archive(493) }
);
source_test!(
    c001_posix_archive_keeps_executable_permissions,
    "tests/test_release_distribution.py::test_posix_archive_keeps_executable_permissions[data]",
    { permission_archive(420) }
);
source_test!(
    c000_mislabeled_shell_architecture_cannot_be_packaged,
    "tests/test_release_distribution.py::test_mislabeled_shell_architecture_cannot_be_packaged[linux-x64-wrong]",
    {
        assert!(release::architecture(b"wrong", "linux-x64").is_err());
        Ok(())
    }
);
source_test!(
    c001_mislabeled_shell_architecture_cannot_be_packaged,
    "tests/test_release_distribution.py::test_mislabeled_shell_architecture_cannot_be_packaged[macos-arm64-wrong]",
    {
        assert!(release::architecture(b"wrong", "macos-arm64").is_err());
        Ok(())
    }
);
source_test!(
    c002_mislabeled_shell_architecture_cannot_be_packaged,
    "tests/test_release_distribution.py::test_mislabeled_shell_architecture_cannot_be_packaged[windows-x64-wrong]",
    {
        assert!(release::architecture(b"wrong", "windows-x64").is_err());
        Ok(())
    }
);
source_test!(
    c000_release_workflow_rejects_failed_forks_and_unapproved_source_refs,
    "tests/test_release_distribution.py::test_release_workflow_rejects_failed_forks_and_unapproved_source_refs",
    {
        let documents = workflow_documents()?;
        release::release_workflows(&documents)?;
        let mut bad = documents.clone();
        bad["ci"]["on"] = json!({"push":{}});
        assert!(release::release_workflows(&bad).is_err());
        let mut bad = documents.clone();
        bad["ci"]["jobs"]["review"]["needs"] = json!(["required"]);
        assert!(release::release_workflows(&bad).is_err());
        for (workflow, job, guard) in [
            ("ci", "coverage", "github.actor == 'someone-else'"),
            ("ci", "attest", "inputs.trusted_sha != github.sha"),
            (
                "release",
                "review",
                "github.repository == 'fork/Numera-Browser'",
            ),
        ] {
            let mut bad = documents.clone();
            bad[workflow]["jobs"][job]["if"] = guard.into();
            assert!(release::release_workflows(&bad).is_err());
        }
        let mut bad = documents.clone();
        bad["ci"]["jobs"]["distribution"]["needs"] = json!(["coverage"]);
        assert!(release::release_workflows(&bad).is_err());
        let mut bad = documents.clone();
        bad["ci"]["jobs"]["distribution"]["secrets"] = "inherit".into();
        assert!(release::release_workflows(&bad).is_err());
        let mut bad = documents.clone();
        bad["ci"]["jobs"]["attest"]["permissions"]["contents"] = "write".into();
        assert!(release::release_workflows(&bad).is_err());
        let mut bad = documents.clone();
        bad["release"]["jobs"]["publish"] = json!({"run":"gh release create fixture"});
        assert!(release::release_workflows(&bad).is_err());
        let mut bad = documents;
        bad["ci"]["jobs"]["attest"]["steps"] =
            json!([{"run":"gh api --method PATCH repos/fixture/git/refs/heads/main"}]);
        assert!(release::release_workflows(&bad).is_err());
        Ok(())
    }
);
source_test!(
    c000_ed25519_exact_manifest_rejects_tampering_wrong_key_and_size,
    "tests/test_release_distribution.py::test_ed25519_exact_manifest_rejects_tampering_wrong_key_and_size",
    { ed25519_exact() }
);
source_test!(
    c000_macos_native_signing_checks_identity_and_discriminates_rejection,
    "tests/test_release_distribution.py::test_macos_native_signing_checks_identity_and_discriminates_rejection",
    { macos_controls() }
);
source_test!(
    c000_immutable_build_inventory_excludes_only_its_own_signature_cycle,
    "tests/test_release_distribution.py::test_immutable_build_inventory_excludes_only_its_own_signature_cycle[executable]",
    { inventory(493) }
);
source_test!(
    c001_immutable_build_inventory_excludes_only_its_own_signature_cycle,
    "tests/test_release_distribution.py::test_immutable_build_inventory_excludes_only_its_own_signature_cycle[data]",
    { inventory(420) }
);
source_test!(
    c000_publisher_reads_owner_signed_build_from_actual_archive,
    "tests/test_release_distribution.py::test_publisher_reads_owner_signed_build_from_actual_archive",
    { archive_build() }
);
source_test!(
    c000_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[None]",
    { codeql_case("None") }
);
source_test!(
    c001_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[scanner-only]",
    { codeql_case("scanner-only") }
);
source_test!(
    c002_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[wrong-sha]",
    { codeql_case("wrong-sha") }
);
source_test!(
    c003_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[failed]",
    { codeql_case("failed") }
);
source_test!(
    c004_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[pending]",
    { codeql_case("pending") }
);
source_test!(
    c005_signing_requires_the_independent_exact_source_codeql_verdict,
    "tests/test_release_distribution.py::test_signing_requires_the_independent_exact_source_codeql_verdict[newer-failure]",
    { codeql_case("newer-failure") }
);
source_test!(
    c000_owner_signing_context_rejects_arbitrary_ref_actor_event_and_sha,
    "tests/test_release_distribution.py::test_owner_signing_context_rejects_arbitrary_ref_actor_event_and_sha",
    {
        let values = BTreeMap::from([
            ("GITHUB_ACTIONS".into(), "true".into()),
            ("GITHUB_EVENT_NAME".into(), "workflow_dispatch".into()),
            ("GITHUB_ACTOR".into(), "KiaroSama".into()),
            ("GITHUB_REF".into(), "refs/heads/main".into()),
            ("GITHUB_SHA".into(), "a".repeat(40)),
            ("NUMERA_TRUSTED_SHA".into(), "a".repeat(40)),
        ]);
        signing::owner_context(&values)?;
        for (key, value) in [
            ("GITHUB_REF", "refs/heads/unreviewed".to_owned()),
            ("GITHUB_ACTOR", "other".to_owned()),
            ("GITHUB_EVENT_NAME", "pull_request".to_owned()),
            ("NUMERA_TRUSTED_SHA", "b".repeat(40)),
        ] {
            let mut bad = values.clone();
            bad.insert(key.into(), value);
            assert!(
                signing::owner_context(&bad)
                    .expect_err("unapproved context")
                    .to_string()
                    .contains("approved manual")
            );
        }
        Ok(())
    }
);
source_test!(
    c000_private_signing_tool_failure_never_displays_captured_secrets,
    "tests/test_release_distribution.py::test_private_signing_tool_failure_never_displays_captured_secrets",
    {
        let request = signing::ToolRequest {
            program: "openssl".into(),
            args: vec!["pkeyutl".into()],
            environment: private_env(),
            input: Vec::new(),
            seconds: 60,
        };
        let result = signing::execute_with(request, 0, None, &mut |request| {
            assert!(
                private_env()
                    .keys()
                    .all(|name| !request.environment.contains_key(name))
            );
            Ok(signing::ToolOutput {
                exit: 1,
                stdout: b"private fixture material".to_vec(),
                stderr: b"private fixture material".to_vec(),
            })
        });
        assert!(
            !result
                .expect_err("failedtool")
                .to_string()
                .contains("private fixture material")
        );
        Ok(())
    }
);
source_test!(
    c000_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[None]",
    { promotion_case("None") }
);
source_test!(
    c001_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[protected]",
    { promotion_case("protected") }
);
source_test!(
    c002_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[rules]",
    { promotion_case("rules") }
);
source_test!(
    c003_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[ruleset]",
    { promotion_case("ruleset") }
);
source_test!(
    c004_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[unknown-rules]",
    { promotion_case("unknown-rules") }
);
source_test!(
    c005_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[diverged]",
    { promotion_case("diverged") }
);
source_test!(
    c006_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[wrong-tip]",
    { promotion_case("wrong-tip") }
);
source_test!(
    c007_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[failed-required]",
    { promotion_case("failed-required") }
);
source_test!(
    c008_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[different-sha]",
    { promotion_case("different-sha") }
);
source_test!(
    c009_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[other-ref]",
    { promotion_case("other-ref") }
);
source_test!(
    c010_embedded_promotion_gate_refuses_unverified_or_unsafe_state,
    "tests/test_release_distribution.py::test_embedded_promotion_gate_refuses_unverified_or_unsafe_state[other-actor]",
    { promotion_case("other-actor") }
);
source_test!(
    c000_package_smoke_pipes_fixture_input_without_plaintext_staging,
    "tests/test_release_distribution.py::test_package_smoke_pipes_fixture_input_without_plaintext_staging[False]",
    { smoke(false) }
);
source_test!(
    c001_package_smoke_pipes_fixture_input_without_plaintext_staging,
    "tests/test_release_distribution.py::test_package_smoke_pipes_fixture_input_without_plaintext_staging[True]",
    { smoke(true) }
);
source_test!(
    c000_manual_ci_secrets_are_named_and_publish_requires_verified_promotion,
    "tests/test_release_distribution.py::test_manual_ci_secrets_are_named_and_publish_requires_verified_promotion",
    {
        let documents = workflow_documents()?;
        release::release_workflows(&documents)?;
        // The user authorized manual signed artifacts for review, so preserve
        // the original source/SHA/verification authority without ref mutation.
        assert!(documents["ci"]["jobs"].get("promote").is_none());
        assert!(documents["ci"]["jobs"].get("publish").is_none());
        assert_eq!(documents["ci"]["jobs"]["attest"]["needs"], "required");
        assert_eq!(
            documents["ci"]["jobs"]["review"]["needs"],
            json!(["required", "attest"])
        );
        let build = workflow::parse(&fs::read_to_string(
            root().join(".github/workflows/build-desktop.yml"),
        )?)?;
        let text = build.to_string();
        for named in [
            "NUMERA_RELEASE_SIGNING_KEY",
            "NUMERA_CODE_SIGNING_PFX",
            "NUMERA_CODE_SIGNING_PASSWORD",
        ] {
            assert!(text.contains(named));
        }
        assert!(!text.contains("secrets: inherit"));
        let needs = documents["ci"]["jobs"]["distribution"]["needs"]
            .as_array()
            .expect("distribution prerequisites");
        for job in ["codeql", "dependencies", "desktop-contracts"] {
            assert!(needs.iter().any(|value| value == job));
        }
        Ok(())
    }
);
