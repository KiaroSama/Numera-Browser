use super::*;

fn built(root: &Path) -> std::path::PathBuf {
    let path = root.join("dist");
    fs::create_dir_all(path.join("assets")).expect("assets");
    fs::write(
        path.join("index.html"),
        "<!doctype html><title>Numera</title>",
    )
    .expect("index");
    fs::write(path.join("assets/index.js"), "export default 1;").expect("script");
    fs::write(path.join("favicon.svg"), "<svg/>").expect("favicon");
    path
}

fn all_files(root: &Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<std::path::PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).expect("state directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                visit(base, &path, out)
            } else {
                out.insert(
                    path.strip_prefix(base).expect("child").to_owned(),
                    fs::read(path).expect("state bytes"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn recovery(root: &Path, ticket: &str, token: &str) -> io::Result<Http> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let sessions = Arc::new(numera_core::ownership::registry::Registry::new(
        root.to_owned(),
        "source-http-host".into(),
        None,
    )?);
    let app = runtime.block_on(numera_core::http::application::router_with_recovery(
        root.to_owned(),
        "",
        "",
        None,
        "source-http-host",
        numera_core::http::application::SessionApplication {
            installed: BTreeMap::new(),
            sessions,
        },
        (None, None),
        numera_core::http::application::RecoveryOptions {
            ticket: Some(ticket.into()),
            update_token: token.into(),
        },
    ))?;
    Ok(Http { runtime, app })
}

struct OfferService {
    failure: Option<io::ErrorKind>,
    calls: std::sync::Mutex<Vec<(std::path::PathBuf, Vec<String>)>>,
}
fn offer_identity() -> Value {
    json!({"engine":"cloak","offer_id":"1".repeat(64),"available":true,"installed":false,"version":"146.0.7680.177.5","reason":null})
}
impl numera_core::http::acquisition_routes::Service for OfferService {
    fn offer(&self) -> io::Result<Value> {
        match self.failure {
            Some(kind) => Err(io::Error::new(kind, "internal-path/fixture-license-value")),
            None => Ok(offer_identity()),
        }
    }
    fn acquire(&self, root: &Path, argv: &[String]) -> io::Result<Value> {
        assert!(
            fs::read_dir(root.join(".updates/work"))?.next().is_some(),
            "HTTP acquisition escaped root admission"
        );
        self.calls
            .lock()
            .expect("acquisition calls")
            .push((root.to_owned(), argv.to_vec()));
        Ok(json!({"installed":true}))
    }
}
fn offer_app(root: &Path, service: Arc<OfferService>) -> Http {
    let app =
        numera_core::http::acquisition_routes::router_with_service(root.to_owned(), "", service);
    Http {
        runtime: tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("acquisition runtime"),
        app,
    }
}
source_test!(
    acquisition_routes_test_http_offer_decline_stale_and_approval_use_exact_owned_cli,
    "tests/test_acquisition_routes.py::test_http_offer_decline_stale_and_approval_use_exact_owned_cli",
    {
        let t = fresh();
        let service = Arc::new(OfferService {
            failure: None,
            calls: Default::default(),
        });
        let h = offer_app(t.path(), service.clone());
        let identity = offer_identity();
        assert_eq!(h.get("/api/engines/cloak/acquisition", None).body, identity);
        assert_eq!(
            h.post(
                "/api/engines/cloak/acquisition",
                json!({"offer_id":identity["offer_id"],"approve":false}),
                None
            )
            .status,
            400
        );
        assert_eq!(
            h.post(
                "/api/engines/cloak/acquisition",
                json!({"offer_id":"2".repeat(64),"approve":true}),
                None
            )
            .status,
            409
        );
        assert!(service.calls.lock().expect("calls").is_empty());
        let response = h.post(
            "/api/engines/cloak/acquisition",
            json!({"offer_id":identity["offer_id"],"approve":true}),
            None,
        );
        assert_eq!(response.status, 200, "{}", response.text());
        let calls = service.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, t.path());
        assert_eq!(
            calls[0].1,
            vec![
                "engines".to_owned(),
                "--acquire-cloak".into(),
                "1".repeat(64),
                "--approve".into(),
                "--json".into()
            ]
        );
        Ok(())
    }
);
source_test!(acquisition_routes_test_acquisition_metadata_errors_do_not_expose_vendor_exception_text_valueerror_192bfb,"tests/test_acquisition_routes.py::test_acquisition_metadata_errors_do_not_expose_vendor_exception_text[ValueError]",{
let t=fresh();let service=Arc::new(OfferService{failure:Some(io::ErrorKind::InvalidData),calls:Default::default()});let h=offer_app(t.path(),service);let response=h.get("/api/engines/cloak/acquisition",None);assert_eq!(response.status,200);assert_eq!(response.body["reason"],"Cloak browser metadata could not be read safely");assert!(!response.text().contains("internal-path"));assert!(!response.text().contains("fixture-license-value"));
Ok(())
});
source_test!(acquisition_routes_test_acquisition_metadata_errors_do_not_expose_vendor_exception_text_oserror_0f7b05,"tests/test_acquisition_routes.py::test_acquisition_metadata_errors_do_not_expose_vendor_exception_text[OSError]",{
let t=fresh();let service=Arc::new(OfferService{failure:Some(io::ErrorKind::Other),calls:Default::default()});let h=offer_app(t.path(),service);let response=h.get("/api/engines/cloak/acquisition",None);assert_eq!(response.status,200);assert_eq!(response.body["reason"],"Cloak browser metadata could not be read safely");assert!(!response.text().contains("internal-path"));assert!(!response.text().contains("fixture-license-value"));
Ok(())
});
source_test!(
    spa_mount_test_root_serves_the_dashboard,
    "tests/test_spa_mount.py::test_root_serves_the_dashboard",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/", None);
        assert_eq!(r.status, 200);
        assert!(r.text().contains("Numera"));
        Ok(())
    }
);
source_test!(
    spa_mount_test_a_client_side_route_falls_back_to_index,
    "tests/test_spa_mount.py::test_a_client_side_route_falls_back_to_index",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/proxies", None);
        assert_eq!(r.status, 200);
        assert!(r.text().to_lowercase().contains("<!doctype html>"));
        Ok(())
    }
);
source_test!(
    spa_mount_test_a_real_file_wins_over_the_fallback,
    "tests/test_spa_mount.py::test_a_real_file_wins_over_the_fallback",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/favicon.svg", None);
        assert_eq!(r.status, 200);
        assert!(r.text().contains("<svg"));
        Ok(())
    }
);
source_test!(
    spa_mount_test_assets_are_served,
    "tests/test_spa_mount.py::test_assets_are_served",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/assets/index.js", None);
        assert_eq!(r.status, 200);
        assert_eq!(r.text(), "export default 1;");
        Ok(())
    }
);
source_test!(
    spa_mount_test_the_api_still_answers,
    "tests/test_spa_mount.py::test_the_api_still_answers",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/api/health", None);
        assert_eq!(r.status, 200);
        assert!(
            r.headers["content-type"]
                .to_str()?
                .starts_with("application/json")
        );
        Ok(())
    }
);
source_test!(
    spa_mount_test_an_unknown_api_path_404s_instead_of_returning_html,
    "tests/test_spa_mount.py::test_an_unknown_api_path_404s_instead_of_returning_html",
    {
        let t = fresh();
        let dist = built(t.path());
        let data = t.path().join("data");
        fs::create_dir(&data)?;
        let h = Http::configured(&data, "", "", Some(&dist));
        let r = h.get("/api/definitely-not-a-real-endpoint", None);
        assert_eq!(r.status, 404);
        assert!(!r.text().to_lowercase().contains("<!doctype html"));
        Ok(())
    }
);
source_test!(
    spa_mount_test_the_app_still_starts_with_no_dashboard_build,
    "tests/test_spa_mount.py::test_the_app_still_starts_with_no_dashboard_build",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        assert_eq!(h.get("/api/health", None).status, 200);
        assert_eq!(h.get("/", None).status, 404);
        Ok(())
    }
);
source_test!(
    spa_mount_test_the_dashboard_is_found_in_a_source_checkout,
    "tests/test_spa_mount.py::test_the_dashboard_is_found_in_a_source_checkout",
    {
        let t = fresh();
        let checkout = t.path().join("checkout");
        let dist = checkout.join("dashboard/dist");
        fs::create_dir_all(&dist)?;
        fs::write(dist.join("index.html"), "<!doctype html>")?;
        let app = checkout.join("engine/numera/server");
        fs::create_dir_all(&app)?;
        assert_eq!(
            numera_core::http::dashboard::discover(&app)
                .expect("checkout dashboard")
                .canonicalize()?,
            dist.canonicalize()?
        );
        Ok(())
    }
);
source_test!(
    spa_mount_test_dashboard_dir_survives_being_moved_deeper,
    "tests/test_spa_mount.py::test_dashboard_dir_survives_being_moved_deeper",
    {
        let t = fresh();
        let checkout = t.path().join("repo");
        let dist = checkout.join("dashboard/dist");
        fs::create_dir_all(&dist)?;
        fs::write(dist.join("index.html"), "<!doctype html>")?;
        let deep = checkout.join("engine/numera/server/sub/deeper");
        fs::create_dir_all(&deep)?;
        assert_eq!(numera_core::http::dashboard::discover(&deep), Some(dist));
        Ok(())
    }
);
source_test!(
    update_startup_test_ordinary_create_app_reserves_store_initialization,
    "tests/test_update_startup.py::test_ordinary_create_app_reserves_store_initialization",
    {
        use numera_core::vault::testing::{self, Action};
        let t = fresh();
        let root = t.path().to_owned();
        let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = observed.clone();
        let fixture = root.clone();
        testing::with_observer(
            move |event| {
                if event.operation == "before-update-store-initialize" {
                    let admission = numera_core::ownership::update_admission::Admission::new(
                        &fixture,
                        "source-http-host",
                    )?;
                    let error = testing::with_observer(
                        |_| Ok(Action::Continue),
                        || admission.quiesce(None, || Ok(false)),
                    )
                    .expect_err("startup work must hold admission");
                    assert!(error.to_string().contains("admitted work"));
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(Action::Continue)
            },
            || {
                let _h = Http::new(&root);
            },
        );
        assert_eq!(observed.load(std::sync::atomic::Ordering::SeqCst), 1);
        let admission =
            numera_core::ownership::update_admission::Admission::new(&root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        admission.resume(&ticket)?;
        Ok(())
    }
);
source_test!(
    update_startup_test_closed_gate_safe_restart_only_serves_ready_until_ticket_resume,
    "tests/test_update_startup.py::test_closed_gate_safe_restart_only_serves_ready_until_ticket_resume",
    {
        let t = fresh();
        let root = t.path();
        let _first = Http::new(root);
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        let before = all_files(root);
        let h = recovery(root, &ticket, "fixture-new-engine-private-token")?;
        assert_eq!(h.get("/api/ready", None).status, 200);
        assert_eq!(h.get("/api/profiles", None).status, 409);
        assert_eq!(all_files(root), before);
        let resumed = h.post(
            "/api/updates/resume",
            json!({"ticket":ticket}),
            Some("fixture-new-engine-private-token"),
        );
        assert_eq!(resumed.status, 200, "{}", resumed.text());
        assert_eq!(h.get("/api/profiles", None).status, 200);
        Ok(())
    }
);
source_test!(
    update_startup_test_unsafe_recovery_startup_refuses_without_store_or_data_changes_vault_bc9427,
    "tests/test_update_startup.py::test_unsafe_recovery_startup_refuses_without_store_or_data_changes[vault]",
    {
        use numera_core::vault::testing::{self, Action};
        let t = fresh();
        let root = t.path();
        let _first = Http::new(root);
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        fs::write(root.join("vault-migration.json"), r#"{"unfinished":true}"#)?;
        let _ = &ticket;
        let before = all_files(root);
        let selected = ticket.clone();
        let result = testing::with_observer(
            |event| {
                assert_ne!(
                    event.operation, "before-update-store-initialize",
                    "unsafe recovery reached mutating store constructor"
                );
                Ok(Action::Continue)
            },
            || recovery(root, &selected, "private-token"),
        );
        let error = result.err().expect("unsafe recovery must fail");
        assert!(error.to_string().contains("recovery startup"), "{error}");
        assert_eq!(all_files(root), before);
        Ok(())
    }
);
source_test!(update_startup_test_unsafe_recovery_startup_refuses_without_store_or_data_changes_adoption_5e6508,"tests/test_update_startup.py::test_unsafe_recovery_startup_refuses_without_store_or_data_changes[adoption]",{
use numera_core::vault::testing::{self,Action};let t=fresh();let root=t.path();let _first=Http::new(root);let admission=numera_core::ownership::update_admission::Admission::new(root,"source-http-host")?;let ticket=admission.quiesce(None,||Ok(false))?;fs::write(root.join("tokens.json"),r#"[{"hash":"fixture"}]"#)?;let _=&ticket;let before=all_files(root);let selected=ticket.clone();let result=testing::with_observer(|event|{assert_ne!(event.operation,"before-update-store-initialize","unsafe recovery reached mutating store constructor");Ok(Action::Continue)},||recovery(root,&selected,"private-token"));let error=result.err().expect("unsafe recovery must fail");assert!(error.to_string().contains("recovery startup"),"{error}");assert_eq!(all_files(root),before);
Ok(())
});
source_test!(update_startup_test_unsafe_recovery_startup_refuses_without_store_or_data_changes_directory_e0cdf8,"tests/test_update_startup.py::test_unsafe_recovery_startup_refuses_without_store_or_data_changes[directory]",{
use numera_core::vault::testing::{self,Action};let t=fresh();let root=t.path();let _first=Http::new(root);let admission=numera_core::ownership::update_admission::Admission::new(root,"source-http-host")?;let ticket=admission.quiesce(None,||Ok(false))?;fs::remove_dir(root.join("dashboard"))?;let _=&ticket;let before=all_files(root);let selected=ticket.clone();let result=testing::with_observer(|event|{assert_ne!(event.operation,"before-update-store-initialize","unsafe recovery reached mutating store constructor");Ok(Action::Continue)},||recovery(root,&selected,"private-token"));let error=result.err().expect("unsafe recovery must fail");assert!(error.to_string().contains("recovery startup"),"{error}");assert_eq!(all_files(root),before);
Ok(())
});
source_test!(
    update_startup_test_unsafe_recovery_startup_refuses_without_store_or_data_changes_ticket_4fe527,
    "tests/test_update_startup.py::test_unsafe_recovery_startup_refuses_without_store_or_data_changes[ticket]",
    {
        use numera_core::vault::testing::{self, Action};
        let t = fresh();
        let root = t.path();
        let _first = Http::new(root);
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        let _ = &ticket;
        let before = all_files(root);
        let selected = "0".repeat(64);
        let result = testing::with_observer(
            |event| {
                assert_ne!(
                    event.operation, "before-update-store-initialize",
                    "unsafe recovery reached mutating store constructor"
                );
                Ok(Action::Continue)
            },
            || recovery(root, &selected, "private-token"),
        );
        let error = result.err().expect("unsafe recovery must fail");
        assert!(error.to_string().contains("recovery startup"), "{error}");
        assert_eq!(all_files(root), before);
        Ok(())
    }
);
source_test!(
    update_startup_test_runtime_serve_safe_recovery_skips_import_and_unneeded_adoption,
    "tests/test_update_startup.py::test_runtime_serve_safe_recovery_skips_import_and_unneeded_adoption",
    {
        use std::{
            process::Stdio,
            time::{Duration, Instant},
        };
        let t = fresh();
        let logs = fresh();
        let root = t.path();
        let _first = Http::new(root);
        fs::write(root.join(".imported"), "nothing to import\n")?;
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        let before = all_files(root);
        let socket = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let port = socket.local_addr()?.port();
        drop(socket);
        let mut command = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
        command
            .arg("--data-dir")
            .arg(root)
            .args([
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--json",
            ])
            .env("NUMERA_UPDATE_TICKET", &ticket)
            .env("NUMERA_UPDATE_TOKEN", "fixture-private-startup-token")
            .env("NUMERA_LOG_DIR", logs.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut tree = numera_core::ownership::tree::OwnedTree::spawn(&mut command)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let result = (|| -> TestResult {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(value) = runtime.block_on(numera_core::benchmark_http::request(
                    &format!("http://127.0.0.1:{port}/api/ready"),
                    "",
                    Duration::from_millis(200),
                )) {
                    assert_eq!(value["ok"], true);
                    break;
                }
                assert!(
                    tree.child_mut().try_wait()?.is_none(),
                    "safe recovery server exited"
                );
                assert!(
                    Instant::now() < deadline,
                    "safe recovery readiness deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(all_files(root), before);
            Ok(())
        })();
        tree.settle(Duration::from_secs(5))?;
        result?;
        admission.resume(&ticket)?;
        Ok(())
    }
);
source_test!(
    update_startup_test_actual_cli_serve_refuses_pending_import_before_opening_store,
    "tests/test_update_startup.py::test_actual_cli_serve_refuses_pending_import_before_opening_store",
    {
        let t = fresh();
        let root = t.path();
        let _first = Http::new(root);
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "source-http-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        let before = all_files(root);
        let output = Command::new(env!("CARGO_BIN_EXE_numera-engine"))
            .arg("--data-dir")
            .arg(root)
            .args(["serve", "--json"])
            .env("NUMERA_UPDATE_TICKET", &ticket)
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        assert!(!root.join(".imported").exists());
        assert_eq!(all_files(root), before);
        admission.resume(&ticket)?;
        Ok(())
    }
);
