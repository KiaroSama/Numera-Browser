use super::*;
use numera_core::{
    ownership::{
        session::Session,
        tree::OwnedTree,
        update_admission::{Admission, GATE, Reservation},
    },
    vault::testing::{self, Action},
};
use std::sync::mpsc;
const HOST: &str = "acceptance-host";
const TOKEN: &str = "fixture-private-update-token";
fn fresh() -> io::Result<common::TempDirectory> {
    common::TempDirectory::new("source-acceptance-update")
}
fn base_command(root: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
    c.arg("--data-dir").arg(root);
    for key in [
        "NUMERA_API_TOKEN",
        "NUMERA_PASSWORD",
        "NUMERA_UPDATE_TOKEN",
        "NUMERA_UPDATE_TICKET",
        "NUMERA_UPDATE_ACTIVATION",
        "NUMERA_BROWSER_RUNTIME",
        "NUMERA_BROWSER_ADAPTER",
        "NUMERA_HELPER_UNITS",
        "NUMERA_HELPER_SCOPE",
    ] {
        c.env_remove(key);
    }
    c
}
fn cli(root: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    base_command(root).args(args).output()
}
fn canceled_worker(authentication: bool) -> TestResult {
    let t = fresh()?;
    let root = t.path();
    let tokens = numera_core::http::tokens::TokenStore::new(root.into());
    let token = if authentication {
        tokens.create("abandoned-auth", 1000.0)?["token"]
            .as_str()
            .ok_or("token missing")?
            .to_owned()
    } else {
        String::new()
    };
    let h = Http::new(root, None)?;
    let original = if authentication {
        Some(fs::read(root.join("tokens.json"))?)
    } else {
        None
    };
    let (entered, started) = mpsc::sync_channel(1);
    let (finish, release) = mpsc::sync_channel(1);
    let mut entered = Some(entered);
    // The real middleware authorizes even an unprotected request in its first
    // blocking worker. The handler case must reach the following worker.
    let stage_to_park = if authentication { 1 } else { 2 };
    let mut blocking_stage = 0;
    let a = Admission::new(root, HOST)?;
    let app = h.app.clone();
    testing::with_observer(
        move |event| {
            // Park the actual admitted worker before it takes an application
            // transaction lock. Quiesce must see active work while the barrier
            // is held, rather than racing its five-second timeout and rollback.
            if event.operation == "before-owned-http-blocking-work" {
                blocking_stage += 1;
            }
            if blocking_stage == stage_to_park
                && event.operation == "before-owned-http-blocking-work"
                && let Some(entered) = entered.take()
            {
                entered.send(blocking_stage).map_err(io::Error::other)?;
                release
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(io::Error::other)?;
            }
            Ok(Action::Continue)
        },
        || {
            h.runtime.block_on(async {
                let mut request = Request::builder()
                    .method(if authentication { "GET" } else { "POST" })
                    .uri("/api/profiles")
                    .header("content-type", "application/json");
                if authentication {
                    request = request.header("authorization", format!("Bearer {token}"));
                }
                let task =
                    tokio::spawn(app.oneshot(request.body(Body::from(if authentication {
                        Vec::new()
                    } else {
                        serde_json::to_vec(&json!({"name":"Slow owned write"}))?
                    }))?));
                let reached = tokio::task::spawn_blocking(move || {
                    started.recv_timeout(Duration::from_secs(5))
                })
                .await?;
                if let Err(error) = reached {
                    let _ = finish.send(());
                    task.abort();
                    return Err(Box::<dyn std::error::Error>::from(error));
                }
                assert_eq!(reached?, stage_to_park, "Wrong HTTP worker was parked");
                task.abort();
                assert!(task.await.is_err());
                let busy = testing::with_observer(
                    |_| Ok(Action::Continue),
                    || a.quiesce(None, || Ok(false)),
                );
                assert!(busy.is_err(), "cancelled await released active IO");
                if let Some(original) = &original {
                    assert_eq!(&fs::read(root.join("tokens.json"))?, original);
                }
                finish.send(())?;
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while fs::read_dir(root.join(".updates/work"))?.next().is_some() {
                    if std::time::Instant::now() >= deadline {
                        return Err("abandoned worker did not settle".into());
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if !authentication {
                    let rows =
                        fs::read_dir(root.join("profiles"))?.collect::<io::Result<Vec<_>>>()?;
                    let profiles = rows
                        .iter()
                        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        profiles.len(),
                        1,
                        "Canceled handler did not finish its actual write"
                    );
                    let profile: Value = serde_json::from_slice(&fs::read(profiles[0].path())?)?;
                    assert_eq!(profile["name"], "Slow owned write");
                }
                let ticket = a.quiesce(None, || Ok(false))?;
                a.resume(&ticket)?;
                Ok(())
            })
        },
    )
}

original!(
    test_gate_blocks_real_session_claim_and_cli_until_exact_ticket_resume,
    "tests/test_update_admission.py::test_gate_blocks_real_session_claim_and_cli_until_exact_ticket_resume",
    {
        let t = fresh()?;
        let root = t.path();
        let admission = Admission::new(root, HOST)?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        let record: Value = serde_json::from_slice(&fs::read(admission.gate())?)?;
        assert_eq!(
            record["ticket_sha256"],
            numera_core::http::tokens::TokenStore::hash(&ticket)
        );
        assert!(!record.to_string().contains(&ticket));
        assert_eq!(
            Session::admit(root, "profile-one", HOST)
                .err()
                .ok_or("gate admitted session")?
                .kind(),
            io::ErrorKind::WouldBlock
        );
        let result = cli(root, &["list", "--json"])?;
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8(result.stdout)?
                .to_lowercase()
                .contains("update")
        );
        assert!(admission.resume(&"0".repeat(64)).is_err());
        admission.resume(&ticket)?;
        drop(Reservation::begin(root, "after resume")?);
        assert!(!admission.gate().exists());

        Ok(())
    }
);
original!(
    test_precommitted_ticket_recovers_after_quiesce_response_is_lost,
    "tests/test_update_admission.py::test_precommitted_ticket_recovers_after_quiesce_response_is_lost",
    {
        let t = fresh()?;
        let root = t.path();
        let ticket = "b".repeat(64);
        Admission::new(root, HOST)?.quiesce(Some(&ticket), || Ok(false))?;
        Admission::new(root, HOST)?.resume(&ticket)?;
        assert!(!root.join(GATE).exists());

        Ok(())
    }
);
original!(
    test_resume_receipt_is_idempotent_but_never_authorizes_another_gate,
    "tests/test_update_admission.py::test_resume_receipt_is_idempotent_but_never_authorizes_another_gate",
    {
        let t = fresh()?;
        let a = Admission::new(t.path(), HOST)?;
        let ticket = a.quiesce(None, || Ok(false))?;
        a.resume(&ticket)?;
        a.resume(&ticket)?;
        assert!(a.resume(&"0".repeat(64)).is_err());
        let next = a.quiesce(None, || Ok(false))?;
        assert!(a.resume(&ticket).is_err());
        assert!(a.gate().exists());
        a.resume(&next)?;

        Ok(())
    }
);
original!(
    test_resume_receipt_failure_keeps_admission_closed,
    "tests/test_update_admission.py::test_resume_receipt_failure_keeps_admission_closed",
    {
        let t = fresh()?;
        let a = Admission::new(t.path(), HOST)?;
        let ticket = a.quiesce(None, || Ok(false))?;
        let result = testing::with_observer(
            |event| {
                if event.operation == "before-update-resume-receipt" {
                    return Err(io::Error::other("controlled receipt failure"));
                }
                Ok(Action::Continue)
            },
            || a.resume(&ticket),
        );
        assert!(
            result
                .err()
                .ok_or("receipt fault swallowed")?
                .to_string()
                .contains("controlled receipt")
        );
        assert!(a.gate().exists());
        a.resume(&ticket)?;

        Ok(())
    }
);
original!(
    test_inflight_work_refuses_quiesce_without_closing_admission,
    "tests/test_update_admission.py::test_inflight_work_refuses_quiesce_without_closing_admission",
    {
        let t = fresh()?;
        let a = Admission::new(t.path(), HOST)?;
        {
            let _work = Reservation::begin(t.path(), "finite helper")?;
            let error = a
                .quiesce(None, || Ok(false))
                .err()
                .ok_or("active work admitted")?;
            assert!(
                error.to_string().contains("active") || error.to_string().contains("uncertain")
            );
        }
        drop(Reservation::begin(t.path(), "still available")?);
        assert!(!a.gate().exists());

        Ok(())
    }
);
original!(
    test_unsettled_helper_reservation_survives_its_parent_scope,
    "tests/test_update_admission.py::test_unsettled_helper_reservation_survives_its_parent_scope",
    {
        let t = fresh()?;
        {
            let mut work = Reservation::begin(t.path(), "stranded helper")?;
            work.retain();
        }
        assert!(
            Admission::new(t.path(), HOST)?
                .quiesce(None, || Ok(false))
                .is_err()
        );
        assert_eq!(fs::read_dir(t.path().join(".updates/work"))?.count(), 1);

        Ok(())
    }
);
original!(
    test_unknown_claim_or_gate_never_expires_into_available,
    "tests/test_update_admission.py::test_unknown_claim_or_gate_never_expires_into_available",
    {
        let t = fresh()?;
        let a = Admission::new(t.path(), HOST)?;
        fs::create_dir(t.path().join("sessions"))?;
        fs::write(t.path().join("sessions/uncertain.json"), b"{")?;
        assert!(a.quiesce(None, || Ok(false)).is_err());
        assert!(!a.gate().exists());
        fs::remove_file(t.path().join("sessions/uncertain.json"))?;
        fs::write(a.gate(), br#"{"created_at":0}"#)?;
        assert!(Reservation::begin(t.path(), "must not run").is_err());

        Ok(())
    }
);
original!(
    test_thread_race_has_no_successful_quiesce_beside_admitted_work,
    "tests/test_update_admission.py::test_thread_race_has_no_successful_quiesce_beside_admitted_work",
    {
        let t = fresh()?;
        let root = t.path().to_owned();
        let a = Admission::new(&root, HOST)?;
        let (ready, started) = mpsc::sync_channel(1);
        let (finish, release) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || -> io::Result<()> {
            let _work = Reservation::begin(&root, "concurrent CLI")?;
            ready.send(()).map_err(io::Error::other)?;
            release
                .recv_timeout(Duration::from_secs(3))
                .map_err(io::Error::other)?;
            Ok(())
        });
        let wait = started.recv_timeout(Duration::from_secs(3));
        let refusal = a.quiesce(None, || Ok(false));
        finish.send(())?;
        worker.join().map_err(|_| "admission worker panicked")??;
        wait?;
        assert!(refusal.is_err());
        let ticket = a.quiesce(None, || Ok(false))?;
        a.resume(&ticket)?;

        Ok(())
    }
);
original!(
    test_actual_session_claim_defers_then_proved_stop_allows_quiesce,
    "tests/test_update_admission.py::test_actual_session_claim_defers_then_proved_stop_allows_quiesce",
    {
        let t = fresh()?;
        let root = t.path();
        let mut command = base_command(root);
        command.arg("mcp").stdin(std::process::Stdio::piped());
        let mut tree = OwnedTree::spawn(&mut command)?;
        let session =
            numera_core::ownership::claims::take(root, "real-session", &tree, HOST, "fixture")?;
        let a = Admission::new(root, HOST)?;
        let refused = a.quiesce(None, || Ok(false));
        assert!(
            refused
                .err()
                .ok_or("live session admitted update")?
                .to_string()
                .contains("browser session")
        );
        tree.settle(Duration::from_secs(3))?;
        let before = fs::read(root.join("sessions/real-session.json"))?;
        assert!(!session.is_empty());
        let ticket = a.quiesce(None, || Ok(false))?;
        a.resume(&ticket)?;
        assert_eq!(fs::read(root.join("sessions/real-session.json"))?, before);
        assert!(numera_core::ownership::claims::release(
            root,
            "real-session",
            &session,
            &mut tree
        )?);

        Ok(())
    }
);
original!(
    test_cancelled_http_await_does_not_release_still_executing_handler,
    "tests/test_update_admission.py::test_cancelled_http_await_does_not_release_still_executing_handler",
    {
        canceled_worker(false)?;

        Ok(())
    }
);
original!(
    test_private_http_token_is_not_public_api_token_and_survives_restart,
    "tests/test_update_admission.py::test_private_http_token_is_not_public_api_token_and_survives_restart",
    {
        let t = fresh()?;
        let root = t.path();
        let h = Http::configured(root, None, "fixture-public-api-token", None, TOKEN)?;
        assert_eq!(h.request("POST", "/api/updates/quiesce", json!({}))?.0, 401);
        assert_eq!(
            h.authorized(
                "POST",
                "/api/updates/quiesce",
                json!({}),
                "fixture-public-api-token"
            )?
            .0,
            401
        );
        assert_eq!(
            h.raw(
                "POST",
                "/api/updates/prepare",
                vec![b' '; 4097],
                &[("authorization", &format!("Bearer {TOKEN}"))]
            )?
            .0,
            400
        );
        let prepared = h.authorized("POST", "/api/updates/prepare", json!({}), TOKEN)?;
        assert_eq!(prepared.0, 200);
        assert_eq!(
            Path::new(prepared.1["root"].as_str().ok_or("root missing")?).canonicalize()?,
            root.canonicalize()?
        );
        let ticket = "a".repeat(64);
        let response = h.authorized(
            "POST",
            "/api/updates/quiesce",
            json!({"ticket":ticket}),
            TOKEN,
        )?;
        assert_eq!(response.0, 200, "{}", response.1);
        assert_eq!(response.1["ticket"], ticket);
        assert_eq!(h.request("GET", "/api/ready", Value::Null)?.0, 200);
        assert_eq!(
            h.authorized(
                "GET",
                "/api/profiles",
                Value::Null,
                "fixture-public-api-token"
            )?
            .0,
            409
        );
        drop(h);
        let h = Http::configured(
            root,
            None,
            "fixture-public-api-token",
            Some(ticket.clone()),
            "fixture-next-engine-token",
        )?;
        assert_eq!(
            h.authorized(
                "POST",
                "/api/updates/resume",
                json!({"ticket":ticket}),
                TOKEN
            )?
            .0,
            401
        );
        assert_eq!(
            h.authorized(
                "POST",
                "/api/updates/resume",
                json!({"ticket":ticket}),
                "fixture-next-engine-token"
            )?
            .0,
            200
        );
        assert_eq!(
            h.authorized(
                "GET",
                "/api/profiles",
                Value::Null,
                "fixture-public-api-token"
            )?
            .0,
            200
        );
        let schema = h.request("GET", "/openapi.json", Value::Null)?.1;
        assert!(schema["paths"].get("/api/updates/quiesce").is_none());
        assert!(schema["paths"].get("/api/updates/prepare").is_none());

        Ok(())
    }
);
original!(
    test_closed_gate_refuses_named_token_auth_before_last_used_write,
    "tests/test_update_admission.py::test_closed_gate_refuses_named_token_auth_before_last_used_write",
    {
        let t = fresh()?;
        let root = t.path();
        let tokens = numera_core::http::tokens::TokenStore::new(root.into());
        let created = tokens.create("fixture-update-auth", 1000.0)?;
        let token = created["token"].as_str().ok_or("token missing")?;
        let h = Http::new(root, None)?;
        let before = fs::read(root.join("tokens.json"))?;
        let a = Admission::new(root, HOST)?;
        let ticket = a.quiesce(None, || Ok(false))?;
        assert_eq!(
            h.authorized("GET", "/api/profiles", Value::Null, token)?.0,
            409
        );
        assert_eq!(fs::read(root.join("tokens.json"))?, before);
        a.resume(&ticket)?;

        Ok(())
    }
);
original!(
    test_abandoned_named_auth_worker_retains_admission_until_last_used_settles,
    "tests/test_update_admission.py::test_abandoned_named_auth_worker_retains_admission_until_last_used_settles",
    {
        canceled_worker(true)?;

        Ok(())
    }
);
original!(
    test_private_update_errors_never_return_internal_paths_or_capabilities_quiesce_fb2347,
    "tests/test_update_admission.py::test_private_update_errors_never_return_internal_paths_or_capabilities[quiesce]",
    {
        let t = fresh()?;
        let root = t.path();
        let h = Http::new(root, None)?;
        let ticket = "a".repeat(64);

        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-atomic-write"
                    && event.path.ends_with(".updates/admission.json")
                {
                    return Err(io::Error::other(
                        "internal-path/fixture-private-recovery-capability",
                    ));
                }
                Ok(Action::Continue)
            },
            || {
                h.authorized(
                    "POST",
                    "/api/updates/quiesce",
                    json!({"ticket":ticket}),
                    TOKEN,
                )
            },
        )?;
        assert_eq!(result.0, 409);
        assert_eq!(
            result.1,
            json!({"detail":"Update state could not be accessed safely","kind":"UpdateBusy"})
        );
        assert!(!result.1.to_string().contains("internal-path"));
        assert!(!result.1.to_string().contains("recovery-capability"));

        Ok(())
    }
);
original!(
    test_private_update_errors_never_return_internal_paths_or_capabilities_resume_e48e1a,
    "tests/test_update_admission.py::test_private_update_errors_never_return_internal_paths_or_capabilities[resume]",
    {
        let t = fresh()?;
        let root = t.path();
        let h = Http::new(root, None)?;
        let ticket = "a".repeat(64);
        Admission::new(root, HOST)?.quiesce(Some(&ticket), || Ok(false))?;
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-update-resume-receipt" {
                    return Err(io::Error::other(
                        "internal-path/fixture-private-recovery-capability",
                    ));
                }
                Ok(Action::Continue)
            },
            || {
                h.authorized(
                    "POST",
                    "/api/updates/resume",
                    json!({"ticket":ticket}),
                    TOKEN,
                )
            },
        )?;
        assert_eq!(result.0, 409);
        assert_eq!(
            result.1,
            json!({"detail":"Update state could not be accessed safely","kind":"UpdateBusy"})
        );
        assert!(!result.1.to_string().contains("internal-path"));
        assert!(!result.1.to_string().contains("recovery-capability"));

        Ok(())
    }
);
original!(
    test_private_token_is_not_inherited_by_browser_helper_children,
    "tests/test_update_admission.py::test_private_token_is_not_inherited_by_browser_helper_children",
    {
        let mut command = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
        for (key, value) in [
            ("NUMERA_UPDATE_TOKEN", TOKEN),
            ("NUMERA_UPDATE_TICKET", "fixture-recovery-ticket"),
            ("NUMERA_UPDATE_ACTIVATION", "fixture-activation-nonce"),
        ] {
            command.env(key, value);
        }
        numera_core::browser::adapter::restrict_environment(&mut command);
        let env = command.get_envs().collect::<BTreeMap<_, _>>();
        for key in [
            "NUMERA_UPDATE_TOKEN",
            "NUMERA_UPDATE_TICKET",
            "NUMERA_UPDATE_ACTIVATION",
        ] {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None));
        }

        Ok(())
    }
);
