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
        root().join(".github/workflows/release-review.yml"),
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
        codeql_analyses_cases(&sha)?;
    } else {
        assert!(result.is_err());
    }
    codeql_source_case(damage)?;
    Ok(())
}
fn codeql_analyses_cases(sha: &str) -> TestResult {
    let categories = ["/language:actions", "/language:rust"];
    let analysis = |id, category| json!({"id":id,"commit_sha":sha,"tool":{"name":"CodeQL"},"category":category,"error":"","results_count":0});
    let clean = json!([analysis(42, categories[0]), analysis(40, categories[1])]);
    assert_eq!(evidence::codeql_analyses(&clean, sha, &categories)?, 42);
    for invalid in [json!({}), json!([clean[0].clone()])] {
        assert!(evidence::codeql_analyses(&invalid, sha, &categories).is_err());
    }
    for wrong_sha in ["b".repeat(40), "invalid".into()] {
        assert!(evidence::codeql_analyses(&clean, &wrong_sha, &categories).is_err());
    }
    for (field, value) in [
        ("results_count", json!(1)),
        ("error", json!("Analysis failed")),
        ("id", json!(0)),
    ] {
        let mut invalid = clean.clone();
        invalid[1][field] = value;
        assert!(evidence::codeql_analyses(&invalid, sha, &categories).is_err());
    }
    let mut newer = analysis(43, categories[0]);
    newer["results_count"] = json!(1);
    assert!(
        evidence::codeql_analyses(
            &json!([newer, clean[0].clone(), clean[1].clone()]),
            sha,
            &categories,
        )
        .is_err()
    );
    assert!(
        evidence::codeql_analyses(
            &json!([
                {"commit_sha":sha,"tool":{"name":"CodeQL"},"category":categories[0]},
                clean[0].clone(),
                clean[1].clone()
            ]),
            sha,
            &categories,
        )
        .is_err()
    );
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
include!("release_archive_controls.rs");
