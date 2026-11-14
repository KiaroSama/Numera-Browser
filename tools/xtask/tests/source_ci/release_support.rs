use super::*;
use ci::release_signing as signing;
use std::io::{Read, Write};
pub(super) fn build() -> ci::Result<(Temp, BTreeMap<String, String>, Value)> {
    let tmp = Temp::new("release-identity")?;
    tmp.write("dashboard/package.json", br#"{"version":"0.36.0"}"#)?;
    for name in ["engine-rust/Cargo.toml", "desktop/src-tauri/Cargo.toml"] {
        tmp.write(name, b"version = \"0.36.0\"\n")?;
    }
    tmp.write("desktop/src-tauri/tauri.conf.json",br#"{"version":"0.36.0","productName":"Numera Browser","bundle":{"publisher":"KiaroSama"}}"#)?;
    let env = BTreeMap::from([
        ("GITHUB_ACTIONS".into(), "true".into()),
        ("GITHUB_SHA".into(), "a".repeat(40)),
        ("GITHUB_RUN_ID".into(), "42".into()),
    ]);
    let record = release::build_identity(&tmp.0, &"a".repeat(40), &env)?;
    Ok((tmp, env, record))
}
pub(super) fn engines(tmp: &Temp) -> TestResult {
    for name in if cfg!(windows) {
        ["numera.exe", "engine/numera-engine.exe"]
    } else {
        ["numera", "engine/numera-engine"]
    } {
        tmp.write(name, b"typed external boundary fixture, never executed")?;
    }
    Ok(())
}
pub(super) fn private_env() -> BTreeMap<String, String> {
    [
        "NUMERA_RELEASE_SIGNING_KEY",
        "NUMERA_CODE_SIGNING_PFX",
        "NUMERA_CODE_SIGNING_PASSWORD",
    ]
    .map(|k| (k.into(), "private fixture material".into()))
    .into()
}
pub(super) fn workflow_documents() -> ci::Result<Value> {
    let ci = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/ci.yml"),
    )?)?;
    let release = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/release.yml"),
    )?)?;
    Ok(json!({"ci":ci,"release":release}))
}
pub(super) fn archive_runtime(extension: &str) -> TestResult {
    let tmp = Temp::new("release-archive")?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir(&stage)?;
    fs::write(stage.join("numera"), b"not executed")?;
    fs::create_dir(stage.join(".updates"))?;
    fs::write(
        stage.join(".updates/activation.json"),
        br#"{"private":"fixture"}"#,
    )?;
    fs::write(stage.join(".numera-update.lock"), b"")?;
    fs::write(
        stage.join(".numera-update-generation.json"),
        br#"{"generation":42}"#,
    )?;
    fs::create_dir(stage.join(format!(".numera-update-{}", "a".repeat(64))))?;
    let target = tmp.0.join(format!("portable{extension}"));
    release::archive_stage(&stage, &target)?;
    let names = if extension == ".zip" {
        let mut packed = zip::ZipArchive::new(fs::File::open(&target)?)?;
        (0..packed.len())
            .map(|i| Ok(packed.by_index(i)?.name()?.into_owned()))
            .collect::<ci::Result<Vec<_>>>()?
    } else {
        let mut packed = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(&target)?));
        packed
            .entries()?
            .map(|entry| Ok(entry?.path()?.to_string_lossy().into_owned()))
            .collect::<ci::Result<Vec<_>>>()?
    };
    assert!(names.contains(&"Numera Browser/numera".into()));
    assert!(
        !names
            .iter()
            .any(|s| s.contains(".updates") || s.contains(".numera-update"))
    );
    Ok(())
}
pub(super) fn permission_archive(mode: u32) -> TestResult {
    let tmp = Temp::new("release-permission")?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir(&stage)?;
    let path = stage.join("numera");
    fs::write(&path, b"not executed fixture")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
    }
    let modified = std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_700);
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(fs::FileTimes::new().set_modified(modified))?;
    let before = fs::metadata(&path)?.modified()?;
    let target = tmp.0.join("portable.tar.gz");
    release::archive_stage_with_mode(&stage, &target, |_, _| Some(mode))?;
    assert_eq!(fs::metadata(&path)?.modified()?, before);
    let mut packed = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(target)?));
    let mut found = false;
    for entry in packed.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == "Numera Browser/numera" {
            found = true;
            assert_eq!(entry.header().mode()? & 0o777, mode);
            let pairs = entry
                .pax_extensions()?
                .ok_or("Exact mtime PAX metadata missing")?
                .map(|e| {
                    let e = e?;
                    Ok((e.key()?.to_owned(), e.value()?.to_owned()))
                })
                .collect::<ci::Result<BTreeMap<_, _>>>()?;
            assert_eq!(pairs["mtime"], "1700000000.123456700");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            assert_eq!(bytes, b"not executed fixture");
        }
    }
    assert!(found);
    Ok(())
}
pub(super) fn promotion_case(damage: &str) -> TestResult {
    let sha = "a".repeat(40);
    let base = "b".repeat(40);
    let mut docs = json!({"branch":{"name":"main","protected":false,"commit":{"sha":base}},"rules":[[]],"rulesets":[[]],"run":{"id":42,"head_sha":sha,"name":"CI","path":".github/workflows/ci.yml","event":"workflow_dispatch","actor":{"login":"KiaroSama"},"head_repository":{"full_name":"KiaroSama/Numera-Browser"},"head_branch":"migration/native-runtime-checkpoint-20261012"},"jobs":[{"jobs":[{"name":"Required verification","conclusion":"success"}]}],"comparison":{"base_commit":{"sha":base},"status":"ahead","commits":[{"sha":sha}]}});
    match damage {
        "protected" => docs["branch"]["protected"] = true.into(),
        "rules" => docs["rules"] = json!([[{"type":"pull_request"}]]),
        "ruleset" => docs["rulesets"] = json!([[{"name":"policy"}]]),
        "unknown-rules" => docs["rules"] = json!([{}]),
        "diverged" => docs["comparison"]["status"] = "diverged".into(),
        "wrong-tip" => docs["comparison"]["commits"][0]["sha"] = base.clone().into(),
        "failed-required" => docs["jobs"][0]["jobs"][0]["conclusion"] = "failure".into(),
        "different-sha" => docs["run"]["head_sha"] = base.clone().into(),
        "other-ref" => docs["run"]["head_branch"] = "unapproved".into(),
        "other-actor" => docs["run"]["actor"]["login"] = "other".into(),
        _ => (),
    };
    let result = signing::promotion(
        &docs,
        &sha,
        42,
        "migration/native-runtime-checkpoint-20261012",
    );
    if damage == "None" {
        assert_eq!(result?, base);
    } else {
        assert!(result.is_err());
    }
    Ok(())
}
pub(super) fn codeql_case(damage: &str) -> TestResult {
    let sha = "a".repeat(40);
    let mut check = json!({"id":42,"name":"CodeQL","app":{"slug":"github-advanced-security"},"head_sha":sha,"status":"completed","conclusion":"success"});
    match damage {
        "scanner-only" => {
            check["name"] = "codeql / CodeQL (rust)".into();
            check["app"]["slug"] = "github-actions".into();
        }
        "wrong-sha" => check["head_sha"] = "b".repeat(40).into(),
        "failed" => check["conclusion"] = "failure".into(),
        "pending" => check["status"] = "in_progress".into(),
        _ => (),
    };
    let mut checks = vec![check.clone()];
    if damage == "newer-failure" {
        check["id"] = 43.into();
        check["conclusion"] = "failure".into();
        checks.push(check);
    }
    let result = evidence::codeql(&json!([{"check_runs":checks}]), &sha);
    if damage == "None" {
        assert_eq!(result?, 42);
    } else {
        assert!(result.is_err());
    }
    codeql_source_case(damage)?;
    Ok(())
}
fn codeql_source_case(damage: &str) -> TestResult {
    use std::time::{Duration, Instant};
    // Real, distinct Git commit objects: the integration commit has both PR
    // parents but exactly the head's source tree. API-shaped fixtures are read
    // through the same production validator used after authenticated gh calls.
    let temp = Temp::new("codeql-source-subject")?;
    let git = |args: &[&str]| -> ci::Result<String> {
        Ok(String::from_utf8(ci::git(&temp.0, args)?)?.trim().into())
    };
    git(&["init", "--quiet"])?;
    temp.write("source.txt", b"base source fixture\n")?;
    git(&["add", "source.txt"])?;
    let base_tree = git(&["write-tree"])?;
    let commit = |tree: &str, parents: &[&str], message: &str| -> ci::Result<String> {
        let mut args = vec![
            "-c",
            "user.name=CodeQL fixture",
            "-c",
            "user.email=codeql-fixture@example.invalid",
            "commit-tree",
            tree,
            "-m",
            message,
        ];
        for parent in parents {
            args.extend(["-p", parent]);
        }
        git(&args)
    };
    let base_sha = commit(&base_tree, &[], "base")?;
    temp.write("source.txt", b"reviewed source fixture\n")?;
    git(&["add", "source.txt"])?;
    let head_tree = git(&["write-tree"])?;
    let head_sha = commit(&head_tree, &[&base_sha], "head")?;
    let build_sha = commit(&head_tree, &[&base_sha, &head_sha], "integration")?;
    git(&["update-ref", "HEAD", &build_sha])?;
    git(&["reset", "--hard", "--quiet", &build_sha])?;
    let context = evidence::CodeqlPrContext {
        repository: "KiaroSama/Numera-Browser".into(),
        event: "pull_request".into(),
        reference: "refs/pull/76/merge".into(),
        number: 76,
        event_head: head_sha.clone(),
        event_base: base_sha.clone(),
        build_sha: build_sha.clone(),
    };
    let local = ci::codeql_local_source(&temp.0, Instant::now() + Duration::from_secs(30))?;
    let pull = json!({"number":76,"state":"open","merge_commit_sha":build_sha,
        "base":{"sha":base_sha,"repo":{"full_name":context.repository}},
        "head":{"sha":head_sha,"repo":{"full_name":context.repository}}});
    let merge = json!({"sha":build_sha,"tree":{"sha":head_tree},
        "parents":[{"sha":base_sha},{"sha":head_sha}]});
    let head = json!({"sha":head_sha,"tree":{"sha":head_tree}});
    let verified = evidence::codeql_pr_subject(&context, &pull, &merge, &head, &local)?;
    assert_ne!(verified.build_commit, verified.analysis_commit);
    assert_eq!(verified.build_commit, build_sha);
    assert_eq!(verified.analysis_commit, head_sha);
    assert_eq!(verified.identical_tree, head_tree);
    assert_ne!(head_tree, base_tree);
    let reject = |c: &evidence::CodeqlPrContext,
                  p: &Value,
                  m: &Value,
                  h: &Value,
                  l: &evidence::CodeqlLocalSource| {
        assert!(evidence::codeql_pr_subject(c, p, m, h, l).is_err());
    };
    match damage {
        "None" => {
            let checks = json!([{"check_runs":[{"id":42,"name":"CodeQL",
                "app":{"slug":"github-advanced-security"},"head_sha":head_sha,
                "status":"completed","conclusion":"success"}]}]);
            assert_eq!(evidence::codeql(&checks, &verified.analysis_commit)?, 42);
            assert!(evidence::codeql(&checks, &verified.build_commit).is_err());
        }
        "scanner-only" => {
            for field in ["repository", "event", "reference"] {
                let mut c = context.clone();
                match field {
                    "repository" => c.repository = "other/Numera-Browser".into(),
                    "event" => c.event = "workflow_dispatch".into(),
                    _ => c.reference = "refs/heads/main".into(),
                }
                reject(&c, &pull, &merge, &head, &local);
            }
            for pointer in ["/base/repo/full_name", "/head/repo/full_name"] {
                let mut p = pull.clone();
                *p.pointer_mut(pointer).ok_or("repository fixture absent")? = "other/repo".into();
                reject(&context, &p, &merge, &head, &local);
            }
        }
        "wrong-sha" => {
            // A genuine merge with an integration-only source difference must
            // refuse, even though both ordered parents are the expected ones.
            let different_sha = commit(&base_tree, &[&base_sha, &head_sha], "changed merge")?;
            let mut c = context.clone();
            c.build_sha = different_sha.clone();
            let mut p = pull.clone();
            p["merge_commit_sha"] = different_sha.clone().into();
            let mut m = merge.clone();
            m["sha"] = different_sha.clone().into();
            m["tree"]["sha"] = base_tree.clone().into();
            git(&["reset", "--hard", "--quiet", &different_sha])?;
            let changed_local =
                ci::codeql_local_source(&temp.0, Instant::now() + Duration::from_secs(30))?;
            reject(&c, &p, &m, &head, &changed_local);
            for field in ["commit", "tree"] {
                let mut l = local.clone();
                if field == "commit" {
                    l.commit = head_sha.clone();
                } else {
                    l.tree = base_tree.clone();
                }
                reject(&context, &pull, &merge, &head, &l);
            }
            let mut c = context.clone();
            c.event_head.replace_range(..1, "A");
            reject(&c, &pull, &merge, &head, &local);
            git(&["reset", "--hard", "--quiet", &build_sha])?;
            git(&["replace", &build_sha, &different_sha])?;
            let replaced = ci::capture(
                std::process::Command::new("git")
                    .env_remove("GIT_NO_REPLACE_OBJECTS")
                    .arg("-C")
                    .arg(&temp.0)
                    .args(["rev-parse", "HEAD^{tree}"]),
                Duration::from_secs(30),
                Duration::from_secs(30),
                4096,
            )?;
            assert_eq!(replaced.exit, 0);
            assert_eq!(String::from_utf8(replaced.stdout)?.trim(), base_tree);
            let actual =
                ci::codeql_local_source(&temp.0, Instant::now() + Duration::from_secs(30))?;
            assert_eq!(actual.tree, head_tree);
            assert!(actual.clean);
            evidence::codeql_pr_subject(&context, &pull, &merge, &head, &actual)?;
        }
        "failed" => {
            for parents in [
                json!([{"sha":head_sha},{"sha":base_sha}]),
                json!([{"sha":base_sha}]),
                json!([{"sha":base_sha},{"sha":head_sha},{"sha":base_sha}]),
                json!([{"sha":head_sha},{"sha":head_sha}]),
                json!([{"sha":base_sha},{"sha":base_sha}]),
            ] {
                let mut m = merge.clone();
                m["parents"] = parents;
                reject(&context, &pull, &m, &head, &local);
            }
            let mut m = merge.clone();
            m.as_object_mut().ok_or("merge object")?.remove("parents");
            reject(&context, &pull, &m, &head, &local);
        }
        "pending" => {
            temp.write("source.txt", b"uncommitted source difference\n")?;
            let dirty = ci::codeql_local_source(&temp.0, Instant::now() + Duration::from_secs(30))?;
            assert!(!dirty.clean);
            reject(&context, &pull, &merge, &head, &dirty);
            reject(&context, &Value::Null, &merge, &head, &local);
            reject(&context, &pull, &Value::Null, &head, &local);
            reject(&context, &pull, &merge, &Value::Null, &local);
            let mut p = pull.clone();
            p["state"] = "closed".into();
            reject(&context, &p, &merge, &head, &local);
            let mut p = pull.clone();
            p["number"] = 77.into();
            reject(&context, &p, &merge, &head, &local);
        }
        "newer-failure" => {
            for pointer in ["/head/sha", "/base/sha", "/merge_commit_sha"] {
                let mut p = pull.clone();
                *p.pointer_mut(pointer).ok_or("freshness fixture absent")? = "b".repeat(40).into();
                reject(&context, &p, &merge, &head, &local);
            }
            let mut m = merge.clone();
            m["sha"] = head_sha.clone().into();
            reject(&context, &pull, &m, &head, &local);
            let mut h = head.clone();
            h["sha"] = base_sha.clone().into();
            reject(&context, &pull, &merge, &h, &local);
            let mut h = head.clone();
            h["tree"]["sha"] = base_tree.into();
            reject(&context, &pull, &merge, &h, &local);
        }
        _ => return ci::fail("unknown CodeQL bridge fixture"),
    }
    Ok(())
}
pub(super) fn macho() -> Vec<u8> {
    let mut bytes = vec![0; 192];
    for (offset, word) in [
        (0, 0xfeedfacfu32),
        (4, 0x0100000c),
        (12, 2),
        (16, 1),
        (20, 16),
        (32, 0x1d),
        (36, 16),
        (40, 128),
        (44, 64),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}
pub(super) fn macos_controls() -> TestResult {
    let tmp = Temp::new("release-macos")?;
    let stage = tmp.0.join("portable path with spaces");
    fs::create_dir_all(stage.join("engine"))?;
    let paths = [stage.join("numera"), stage.join("engine/numera-engine")];
    let content = macho();
    for p in &paths {
        fs::write(p, &content)?;
    }
    let certificate = signing::Certificate {
        sha256: ci::sha(b"synthetic public certificate fixture"),
        sha1: "a".repeat(40),
    };
    let approved = format!("=certificate leaf = H\"{}\"", certificate.sha1);
    let wrong = format!("=certificate leaf = H\"0{}\"", &certificate.sha1[1..]);
    let modes = [
        ("specific-mismatch", 3),
        ("accepted-wrong-signer", 0),
        ("invalid-arguments", 2),
        ("other-verification-failure", 1),
        ("wrong-signer-timeout", 3),
        ("wrong-signer-missing-tool", 3),
        ("accepted-tamper", 3),
        ("tamper-timeout", 3),
        ("tamper-tool-error", 3),
        ("tamper-invalid-arguments", 3),
        ("tamper-missing-tool", 3),
    ];
    for (mode, mismatch_exit) in modes {
        let metadata = stage.join("SIGNING.json");
        if metadata.exists() {
            fs::remove_file(&metadata)?;
        }
        let mut calls = Vec::new();
        let result = signing::native_macos_with(
            &stage,
            &tmp.0.join("private staging"),
            &certificate,
            b"synthetic owned transport",
            &private_env(),
            |request| {
                calls.push(request.clone());
                assert_eq!(request.seconds, 60);
                assert!(
                    private_env()
                        .keys()
                        .all(|name| !request.environment.contains_key(name))
                );
                let mut code = 0;
                let mut diagnostic = b"private fixture tool detail".to_vec();
                if request.program == "codesign" && request.args.iter().any(|a| a == "--verify") {
                    let index = request
                        .args
                        .iter()
                        .position(|a| a == "--test-requirement")
                        .expect("strict requirement");
                    assert!(request.args.iter().any(|a| a == "--strict"));
                    let requirement = request.args[index + 1].to_str().expect("requirement");
                    assert!([approved.as_str(), wrong.as_str()].contains(&requirement));
                    let path = PathBuf::from(request.args.last().expect("path"));
                    if requirement == wrong {
                        assert!(paths.contains(&path));
                        if mode == "wrong-signer-timeout" {
                            return Err(io::Error::new(io::ErrorKind::TimedOut, "private fixture"));
                        }
                        if mode == "wrong-signer-missing-tool" {
                            return Err(io::Error::new(io::ErrorKind::NotFound, "private fixture"));
                        }
                        code = mismatch_exit;
                    } else if path
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with("tampered-"))
                    {
                        let mut damaged = content.clone();
                        damaged[127] ^= 1;
                        assert_eq!(fs::read(&path)?, damaged);
                        assert_eq!(request.environment["LC_ALL"], "C");
                        if mode == "tamper-timeout" {
                            return Err(io::Error::new(io::ErrorKind::TimedOut, "private fixture"));
                        }
                        if mode == "tamper-missing-tool" {
                            return Err(io::Error::new(io::ErrorKind::NotFound, "private fixture"));
                        }
                        code = if mode == "accepted-tamper" {
                            0
                        } else if mode == "tamper-invalid-arguments" {
                            2
                        } else {
                            1
                        };
                        if mode != "tamper-tool-error" {
                            diagnostic
                                .extend_from_slice(b"\nfixture: code or signature modified\n");
                        }
                    } else {
                        assert!(paths.contains(&path));
                    }
                } else {
                    assert!(
                        request.program == "security" || request.args.iter().any(|a| a == "--sign")
                    );
                }
                Ok(signing::ToolOutput {
                    exit: code,
                    stdout: b"private fixture stdout".to_vec(),
                    stderr: diagnostic,
                })
            },
        );
        if mode == "specific-mismatch" {
            result?;
            let record: Value = serde_json::from_slice(&fs::read(&metadata)?)?;
            assert_eq!(record["certificate_sha256"], certificate.sha256);
            assert_eq!(record["trust"], "owner-self-signed-private-trust");
            assert_eq!(record["notarized"], false);
            assert_eq!(record["publicly_trusted"], false);
            for p in &paths {
                let verify = calls
                    .iter()
                    .filter(|c| {
                        c.program == "codesign"
                            && c.args.iter().any(|a| a == "--verify")
                            && c.args.last().is_some_and(|a| a == p.as_os_str())
                    })
                    .map(|c| {
                        let index = c
                            .args
                            .iter()
                            .position(|a| a == "--test-requirement")
                            .expect("index");
                        c.args[index + 1].to_string_lossy().into_owned()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    verify,
                    [
                        approved.clone(),
                        wrong.clone(),
                        approved.clone(),
                        approved.clone()
                    ]
                );
            }
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| c.args.last().is_some_and(|a| Path::new(a)
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with("tampered-"))))
                    .count(),
                2
            );
        } else {
            let error = result.expect_err("failed native control must refuse");
            assert!(!error.to_string().contains("private fixture"));
            assert!(!metadata.exists());
            if mode.ends_with("timeout") {
                assert!(error.to_string().contains("exceeded its deadline"));
            } else if mode.ends_with("missing-tool") {
                assert_eq!(
                    error
                        .downcast_ref::<io::Error>()
                        .expect("typed missing tool")
                        .kind(),
                    io::ErrorKind::NotFound
                );
            } else {
                let exit = if mode == "accepted-tamper" {
                    0
                } else if mode == "tamper-invalid-arguments" {
                    2
                } else if mode == "tamper-tool-error" {
                    1
                } else {
                    mismatch_exit
                };
                assert!(error.to_string().contains(&format!("exit={exit}")));
            }
        }
        assert!(
            calls
                .iter()
                .any(|c| c.program == "security" && c.args[0] == "delete-keychain")
        );
        assert_eq!(fs::read_dir(tmp.0.join("private staging"))?.count(), 0);
        for p in &paths {
            assert_eq!(fs::read(p)?, content);
        }
    }
    Ok(())
}
pub(super) fn ed25519_exact() -> TestResult {
    use ed25519_dalek::{Signature, SigningKey, Verifier};
    let tmp = Temp::new("release-ed25519")?;
    let manifest = tmp.write(
        "update-manifest.json",
        b"{\"schema\":1,\"generation\":42}\n",
    )?;
    let key = SigningKey::from_bytes(&[7; 32]);
    let public = key.verifying_key();
    let signature = tmp.0.join("update-manifest.sig");
    signing::sign_manifest(&manifest, &signature, &key, &public)?;
    let proof = fs::read(&signature)?;
    assert_eq!(proof.len(), 64);
    signing::verify_manifest(&manifest, &signature, &public)?;
    public.verify(&fs::read(&manifest)?, &Signature::from_slice(&proof)?)?;
    assert!(
        signing::sign_manifest(&manifest, &signature, &key, &public)
            .expect_err("exclusive destination")
            .to_string()
            .contains("destination new")
    );
    assert_eq!(fs::read(&signature)?, proof);
    let original = fs::read(&manifest)?;
    fs::write(&manifest, b"{\"schema\":1,\"generation\":43}\n")?;
    assert!(
        signing::verify_manifest(&manifest, &signature, &public)
            .expect_err("tamper")
            .to_string()
            .contains("signature")
    );
    fs::write(&manifest, &original)?;
    let wrong = SigningKey::from_bytes(&[8; 32]);
    let refused = tmp.0.join("wrong-signer.sig");
    assert!(
        signing::sign_manifest(&manifest, &refused, &wrong, &public)
            .expect_err("wrong signer")
            .to_string()
            .contains("pinned public signer")
    );
    assert!(!refused.exists());
    assert_eq!(fs::read(&signature)?, proof);
    assert!(signing::verify_manifest(&manifest, &signature, &wrong.verifying_key()).is_err());
    fs::write(&signature, [b'x'; 63])?;
    assert!(
        signing::verify_manifest(&manifest, &signature, &public)
            .expect_err("wrongsize")
            .to_string()
            .contains("size")
    );
    assert!(
        !fs::read_dir(&tmp.0)?
            .any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with("numera-sign-")))
    );
    Ok(())
}
pub(super) fn inventory(mode: u32) -> TestResult {
    let tmp = Temp::new("release-inventory")?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir(&stage)?;
    let executable = stage.join("numera");
    fs::write(&executable, b"fixture executable, never run")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(mode))?;
    }
    fs::write(stage.join("LICENSE"), b"fixture terms")?;
    fs::create_dir(stage.join(".updates"))?;
    fs::write(stage.join(".updates/runtime.lock"), b"")?;
    fs::write(
        stage.join(".updates/activation.json"),
        br#"{"private":"fixture"}"#,
    )?;
    let build = signing::installed_manifest_with_mode(
        &stage,
        "linux-x64",
        &"a".repeat(40),
        42,
        "0.36.0",
        json!({"sdk":{},"browsers":{}}),
        |path| Ok(path == executable && mode & 0o111 != 0),
    )?;
    assert_eq!(
        build
            .as_object()
            .expect("build fields")
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "schema",
            "owner",
            "product",
            "version",
            "source_sha",
            "generation",
            "platform",
            "minimum_protocol",
            "components",
            "files"
        ])
    );
    assert_eq!(build["owner"], "KiaroSama");
    assert_eq!(build["generation"], 42);
    let rows = build["files"]
        .as_array()
        .expect("inventory")
        .iter()
        .map(|r| (r["path"].as_str().expect("path"), r))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        rows.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from(["numera", "LICENSE"])
    );
    assert_eq!(rows["numera"]["sha256"], ci::digest(&executable)?);
    assert_eq!(rows["numera"]["executable"], mode & 0o111 != 0);
    fs::create_dir(stage.join("data"))?;
    fs::write(stage.join("data/profile.json"), b"{}")?;
    assert!(
        signing::installed_manifest(
            &stage,
            "linux-x64",
            &"a".repeat(40),
            42,
            "0.36.0",
            json!({})
        )
        .expect_err("data must refuse publication")
        .to_string()
        .contains("publication")
    );
    Ok(())
}
pub(super) fn archive_build() -> TestResult {
    use ed25519_dalek::SigningKey;
    let tmp = Temp::new("release-archive-build")?;
    let build = json!({"generation":42,"source_sha":"a".repeat(40),"platform":"windows-x64","components":{"sdk":{},"browsers":{}}});
    let manifest = tmp.write("BUILD.json", &serde_json::to_vec(&build)?)?;
    let signature = tmp.0.join("BUILD.sig");
    let key = SigningKey::from_bytes(&[9; 32]);
    signing::sign_manifest(&manifest, &signature, &key, &key.verifying_key())?;
    let archive = tmp.0.join("numera-windows.zip");
    let mut packed = zip::ZipWriter::new(fs::File::create(&archive)?);
    for (path, name) in [
        (&manifest, "Numera Browser/BUILD.json"),
        (&signature, "Numera Browser/BUILD.sig"),
    ] {
        packed.start_file(name, zip::write::SimpleFileOptions::default())?;
        packed.write_all(&fs::read(path)?)?;
    }
    packed.finish()?;
    let mut metadata = json!({"build_manifest_sha256":ci::digest(&manifest)?,"components":build["components"],"max_members":2,"expanded_size":fs::metadata(&manifest)?.len()+fs::metadata(&signature)?.len()});
    signing::validate_archive_build(
        &archive,
        &metadata,
        "windows-x64",
        42,
        &"a".repeat(40),
        &key.verifying_key(),
    )?;
    assert!(
        signing::validate_archive_build(
            &archive,
            &metadata,
            "windows-x64",
            43,
            &"a".repeat(40),
            &key.verifying_key()
        )
        .expect_err("generation")
        .to_string()
        .contains("generation")
    );
    metadata["build_manifest_sha256"] = "0".repeat(64).into();
    assert!(
        signing::validate_archive_build(
            &archive,
            &metadata,
            "windows-x64",
            42,
            &"a".repeat(40),
            &key.verifying_key()
        )
        .expect_err("hash")
        .to_string()
        .contains("hash/signature")
    );
    Ok(())
}
pub(super) fn bundle(kind: &str) -> TestResult {
    let tmp = Temp::new("release-bundle")?;
    let app = tmp.0.join("Numera/engine");
    fs::create_dir_all(&app)?;
    let root = tmp.0.join("Numera/browsers");
    let mut env = BTreeMap::new();
    if kind != "missing" {
        fs::create_dir_all(&root)?;
    }
    if ["present", "explicit"].contains(&kind) {
        fs::create_dir(root.join("chromium-1234"))?;
        fs::write(
            root.join("chromium-1234/chrome"),
            b"owned browser fixture, never executed",
        )?;
        fs::write(
            root.join("manifest.json"),
            br#"{"playwright":{"executable":"chromium-1234/chrome"}}"#,
        )?;
    }
    if kind == "explicit" {
        env.insert(
            "PLAYWRIGHT_BROWSERS_PATH".into(),
            "/somewhere/chosen".into(),
        );
    }
    let before = env.clone();
    let found = numera_core::browser::bundled::discover(&app)?;
    let result = release::bundled_browser(&app, &tmp.0.join("data"), &env)?;
    assert_eq!(env, before, "resource discovery changed caller environment");
    if kind == "present" {
        assert_eq!(found, Some(root));
        assert!(result.options.contains_key("executable_path"));
        assert!(
            result.options["executable_path"]
                .as_str()
                .expect("browser path")
                .contains("chromium-1234")
        );
    } else {
        assert!(result.options.is_empty());
        if kind == "explicit" {
            assert_eq!(env["PLAYWRIGHT_BROWSERS_PATH"], "/somewhere/chosen");
        } else {
            assert!(found.is_none());
            assert!(!env.contains_key("PLAYWRIGHT_BROWSERS_PATH"));
        }
    }
    Ok(())
}
pub(super) fn smoke(timeout: bool) -> TestResult {
    use std::{
        ffi::OsString,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::Duration,
    };
    const MARKER: &str = "NUMERA_RELEASE_SMOKE_TEST";
    let name = std::thread::current().name().expect("test name").to_owned();
    if std::env::var(MARKER).as_deref() == Ok(name.as_str()) {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input)?;
        let mut peer = TcpStream::connect(std::env::var("NUMERA_RELEASE_SMOKE_SOCKET")?)?;
        peer.write_all(&input)?;
        peer.shutdown(std::net::Shutdown::Write)?;
        if timeout {
            std::thread::sleep(Duration::from_secs(30));
        } else {
            std::io::stdout().write_all(br#"{"ok":true}"#)?;
        }
        return Ok(());
    }
    let tmp = Temp::new("release-smoke")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let mut env = std::env::vars().collect::<BTreeMap<_, _>>();
    env.insert(MARKER.into(), name.clone());
    env.insert(
        "NUMERA_RELEASE_SMOKE_SOCKET".into(),
        listener.local_addr()?.to_string(),
    );
    let argv = ["--exact", &name, "--nocapture", "--test-threads=1"].map(OsString::from);
    let input = b"public fixture \xce\xb1\n";
    let mut received = Vec::new();
    let output = signing::smoke_with(
        &std::env::current_exe()?,
        &argv,
        input,
        &env,
        if timeout { 1 } else { 5 },
        |request| {
            assert_eq!(request.input, input);
            let mut result = signing::execute(request);
            if let Ok(output) = &mut result {
                let observed = output
                    .stdout
                    .windows(b"{\"ok\":true}".len())
                    .find(|part| *part == b"{\"ok\":true}")
                    .ok_or_else(|| {
                        io::Error::other("Native fixture did not emit its JSON result")
                    })?;
                output.stdout = observed.to_vec();
            }
            let end = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut peer, _)) => {
                        peer.set_read_timeout(Some(Duration::from_secs(1)))?;
                        peer.read_to_end(&mut received)?;
                        break;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < end =>
                    {
                        std::thread::sleep(Duration::from_millis(3))
                    }
                    Err(e) => return Err(e),
                }
            }
            result
        },
    );
    if timeout {
        assert!(
            output
                .expect_err("native timeout")
                .to_string()
                .contains("deadline")
        );
    } else {
        assert_eq!(output?, json!({"ok":true}));
    }
    assert_eq!(received, input);
    assert!(!tmp.0.join("fixture-proxies.txt").exists());
    Ok(())
}

pub(super) fn windows_signing_fixture() -> TestResult {
    pe_digest_control_boundaries()?;
    #[cfg(windows)]
    {
        windows_signing_native_fixture()
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

fn pe_digest_control_boundaries() -> TestResult {
    use numera_core::native_windows_sign::digest_control_offset;
    fn put16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let temp = Temp::new("signed-pe-control-boundaries")?;
    let path = temp.0.join("structural-only-fixture.exe");
    for (magic, optional_size, directories) in [(0x20b, 240, 112), (0x10b, 224, 96)] {
        // Hand-laid-out PE headers, not a cryptographically signed executable.
        // Only the Windows branch below establishes real signature behavior.
        let mut bytes = vec![0u8; 0x900];
        bytes[..2].copy_from_slice(b"MZ");
        put32(&mut bytes, 0x3c, 0x100);
        bytes[0x100..0x104].copy_from_slice(b"PE\0\0");
        put16(&mut bytes, 0x106, 2);
        put16(&mut bytes, 0x114, optional_size);
        put16(&mut bytes, 0x116, 2);
        let optional = 0x118;
        put16(&mut bytes, optional, magic);
        put32(&mut bytes, optional + 60, 0x400);
        put32(&mut bytes, optional + directories - 4, 16);
        put32(&mut bytes, optional + directories + 32, 0x800);
        put32(&mut bytes, optional + directories + 36, 0x100);
        let table = optional + usize::from(optional_size);
        for (index, raw, flags) in [(0, 0x400, 0x40000040), (1, 0x600, 0x60000020)] {
            let section = table + index * 40;
            put32(&mut bytes, section + 16, 0x200);
            put32(&mut bytes, section + 20, raw);
            put32(&mut bytes, section + 36, flags);
        }
        fs::write(&path, &bytes)?;
        assert_eq!(
            digest_control_offset(&path)?,
            0x600,
            "select actual executable bytes, not DOS/header data"
        );
        assert_eq!(fs::read(&path)?, bytes, "selector must be read-only");
        let executable = table + 40;
        let mut rejected = Vec::new();
        for (offset, value) in [
            (0x3c, u32::MAX),
            (optional + 60, 0x200),
            (optional + directories - 4, 4),
            (optional + directories - 4, u32::MAX),
            (optional + directories + 32, 0),
            (optional + directories + 32, 0x200),
            (optional + directories + 36, 0),
            (optional + directories + 36, u32::MAX),
            (table + 16, 0x300),
            (executable + 16, 0),
            (executable + 20, 0x200),
            (executable + 20, 0x780),
            (executable + 20, 0x880),
            (executable + 20, u32::MAX - 15),
            (executable + 36, 0x40000020),
            (executable + 36, 0x60000040),
        ] {
            let mut changed = bytes.clone();
            put32(&mut changed, offset, value);
            rejected.push(changed);
        }
        for (offset, value) in [
            (0x106, 0),
            (0x106, 97),
            (0x114, 2),
            (0x116, 0),
            (optional, 0),
        ] {
            let mut changed = bytes.clone();
            put16(&mut changed, offset, value);
            rejected.push(changed);
        }
        rejected.push(bytes[..0x100].to_vec());
        let mut bad_signature = bytes.clone();
        bad_signature[0x100] ^= 1;
        rejected.push(bad_signature);
        let mut bad_dos = bytes.clone();
        bad_dos[0] ^= 1;
        rejected.push(bad_dos);
        let mut unaligned_certificate = bytes.clone();
        put32(
            &mut unaligned_certificate,
            optional + directories + 32,
            0x809,
        );
        put32(
            &mut unaligned_certificate,
            optional + directories + 36,
            0x80,
        );
        rejected.push(unaligned_certificate);
        for (index, changed) in rejected.into_iter().enumerate() {
            fs::write(&path, &changed)?;
            assert!(
                digest_control_offset(&path).is_err(),
                "malformed PE control {magic:x}/{index} admitted"
            );
            assert_eq!(fs::read(&path)?, changed, "refused selector changed bytes");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_signing_native_fixture() -> TestResult {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::native_windows_sign;
    let provenance: Value =
        serde_json::from_str(include_str!("../fixtures/windows-signing/provenance.json"))?;
    let pfx = include_bytes!("../fixtures/windows-signing/public-fixture.pfx");
    let der = include_bytes!("../fixtures/windows-signing/public-fixture.der");
    let pin = ci::sha(der);
    assert_eq!(
        ci::sha(pfx),
        provenance["files"]["public-fixture.pfx"]["sha256"]
    );
    assert_eq!(pin, provenance["files"]["public-fixture.der"]["sha256"]);
    // The public fixture cannot authenticate an owner publication.
    let owner_pem = fs::read_to_string(root().join("docs/code-signing-cert.pem"))?;
    let owner_der = STANDARD.decode(
        owner_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>(),
    )?;
    assert_ne!(pin, ci::sha(&owner_der));
    let engine = std::env::var_os("NUMERA_TEST_SIGNING_ENGINE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("engine-rust/target/release/numera-engine.exe"));
    assert!(
        engine.is_file(),
        "Windows signing coverage requires the freshly built native numera-engine.exe"
    );
    let original_hash = ci::digest(&engine)?;
    let worker = engine
        .parent()
        .ok_or("Native engine build directory missing")?
        .join("numera-browser-runtime.exe");
    assert!(
        worker.is_file(),
        "Windows signing coverage requires the freshly built native SDK worker"
    );
    let worker_hash = ci::digest(&worker)?;
    let environment = std::env::vars().collect();
    let identity = signing::execute_with(
        signing::ToolRequest {
            program: engine.to_str().ok_or("Engine path not Unicode")?.into(),
            args: vec!["--build-info".into()],
            environment,
            input: Vec::new(),
            seconds: 15,
        },
        0,
        None,
        &mut signing::execute,
    )?;
    let identity: Value = serde_json::from_slice(&identity)?;
    assert_eq!(identity["owner"], "KiaroSama");
    assert_eq!(identity["product"], "Numera Browser");
    assert_eq!(identity["application_runtime"], "native");
    assert_eq!(identity["acceptance"], false);
    assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
    if let Ok(expected) = std::env::var("GITHUB_SHA") {
        assert_eq!(identity["source_sha"], expected);
    }
    let tmp = Temp::new("public-windows-signing")?;
    native_windows_sign::indirect_digest_controls(&tmp.0)?;
    let stage = tmp.0.join("Numera Browser");
    fs::create_dir_all(stage.join("engine"))?;
    let paths = [
        stage.join("numera.exe"),
        stage.join("engine/numera-engine.exe"),
        stage.join("engine/numera-browser-runtime.exe"),
    ];
    for path in &paths[..2] {
        fs::copy(&engine, path)?;
    }
    fs::copy(&worker, &paths[2])?;
    let receipt = native_windows_sign::sign(
        &stage,
        pfx,
        provenance["password"]
            .as_str()
            .ok_or("Public fixture password missing")?,
        &pin,
        env!("CARGO_PKG_VERSION"),
    )?;
    assert_eq!(receipt["certificate_sha256"], pin);
    assert_eq!(receipt["owner"], "KiaroSama");
    assert_eq!(receipt["trust"], "owner-self-signed-private-trust");
    assert_eq!(receipt["notarized"], false);
    assert_eq!(receipt["publicly_trusted"], false);
    assert_eq!(
        receipt["files"]
            .as_array()
            .ok_or("Signed native file list missing")?
            .len(),
        3
    );
    let wrong = format!(
        "{}{}",
        if pin.starts_with('0') { '1' } else { '0' },
        &pin[1..]
    );
    for (index, path) in paths.iter().enumerate() {
        native_windows_sign::verify(path, &pin)?;
        let signed_hash = ci::digest(path)?;
        assert!(native_windows_sign::verify(path, &wrong).is_err());
        native_windows_sign::verify(path, &pin)?;
        let damaged = tmp.0.join(format!("tampered-{index}.exe"));
        let mut bytes = fs::read(path)?;
        assert!(bytes.len() > 64);
        bytes[64] ^= 1;
        fs::write(&damaged, &bytes)?;
        assert!(native_windows_sign::verify(&damaged, &pin).is_err());
        assert_eq!(ci::digest(path)?, signed_hash);
    }
    assert_eq!(
        ci::digest(&engine)?,
        original_hash,
        "fixture signing modified the build artifact"
    );
    assert_eq!(
        ci::digest(&worker)?,
        worker_hash,
        "fixture signing modified the native SDK build artifact"
    );
    Ok(())
}
