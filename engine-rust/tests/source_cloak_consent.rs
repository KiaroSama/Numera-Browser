//! Nine individually reported native ports of tests/test_cloak_consent.py.
//! Vendor IO is the typed boundary; consent, leases, parser and HTTP validation
//! remain production code. Launch environment is returned to its owned child,
//! rather than mutating the process-global Python environment.
use numera_core::{
    cloak_acquisition::{self, Acquisition, LicenseInfo, Metadata, Vendor},
    storage::locks::SessionLease,
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
mod common;
type TestResult = Result<(), Box<dyn std::error::Error>>;
macro_rules! source_test {
    ($name:ident,$source:literal,$body:block)=>{ #[test] #[doc=$source] fn $name()->TestResult $body };
}
const VERSION: &str = "146.0.7680.177.5";

struct LocalVendor {
    metadata: Mutex<Metadata>,
    calls: Mutex<Vec<Value>>,
    license: Mutex<Option<LicenseInfo>>,
    fail: AtomicBool,
    reentrant: Mutex<Option<Weak<Acquisition>>>,
    overlaps: AtomicUsize,
}
impl LocalVendor {
    fn new(root: &Path) -> Self {
        let cache = root.join("vendor-cache");
        Self {
            metadata: Mutex::new(Metadata {
                sdk: "0.5.12".into(),
                version: VERSION.into(),
                platform: "windows-x64".into(),
                binary: cache.join("chrome"),
                cache,
                license_key: None,
                requested: false,
                custom_mirror: false,
                binary_override: None,
                unavailable: None,
            }),
            calls: Mutex::new(Vec::new()),
            license: Mutex::new(Some(LicenseInfo {
                valid: true,
                plan: "solo".into(),
            })),
            fail: AtomicBool::new(false),
            reentrant: Mutex::new(None),
            overlaps: AtomicUsize::new(0),
        }
    }
    fn patch(&self, change: impl FnOnce(&mut Metadata)) {
        change(&mut self.metadata.lock().expect("metadata fixture"));
    }
    fn calls(&self) -> Vec<Value> {
        self.calls.lock().expect("download fixture calls").clone()
    }
}
impl Vendor for LocalVendor {
    fn metadata(&self) -> io::Result<Metadata> {
        Ok(self.metadata.lock().expect("metadata fixture").clone())
    }
    fn license(&self, _: &Metadata, _: &str, _: &AtomicBool) -> io::Result<Option<LicenseInfo>> {
        Ok(self.license.lock().expect("license fixture").clone())
    }
    fn ensure_binary(&self, metadata: &Metadata, _: &AtomicBool) -> io::Result<PathBuf> {
        if self.fail.load(Ordering::Acquire) {
            return Err(io::Error::other("controlled vendor failure"));
        }
        if let Some(acquisition) = self
            .reentrant
            .lock()
            .expect("reentrant fixture")
            .as_ref()
            .and_then(Weak::upgrade)
        {
            let offer = acquisition.offer();
            let error = acquisition
                .acquire(
                    offer["offer_id"].as_str().expect("offer ID"),
                    true,
                    &AtomicBool::new(false),
                )
                .expect_err("reentrant acquisition was allowed");
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
            assert!(
                error.to_string().contains("acquisition")
                    && error.to_string().contains("in progress")
            );
            self.overlaps.fetch_add(1, Ordering::AcqRel);
        }
        let mut call = json!({"browser_version":metadata.version});
        if let Some(key) = &metadata.license_key {
            call["license_key"] = key.clone().into();
        }
        self.calls
            .lock()
            .expect("download fixture calls")
            .push(call);
        fs::create_dir_all(
            metadata
                .binary
                .parent()
                .ok_or_else(|| io::Error::other("fixture binary parent"))?,
        )?;
        fs::write(&metadata.binary, b"controlled SDK output")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&metadata.binary, fs::Permissions::from_mode(0o700))?;
        }
        Ok(metadata.binary.clone())
    }
}
struct Fixture {
    temporary: common::TempDirectory,
    vendor: Arc<LocalVendor>,
    acquisition: Arc<Acquisition>,
}
impl Fixture {
    fn new(name: &str) -> io::Result<Self> {
        let temporary = common::TempDirectory::new(name)?;
        let vendor = Arc::new(LocalVendor::new(temporary.path()));
        let acquisition = Arc::new(Acquisition::new(vendor.clone()));
        Ok(Self {
            temporary,
            vendor,
            acquisition,
        })
    }
    fn id(&self) -> String {
        self.acquisition.offer()["offer_id"]
            .as_str()
            .expect("fixture offer ID")
            .into()
    }
    fn binary(&self) -> PathBuf {
        self.vendor
            .metadata
            .lock()
            .expect("metadata fixture")
            .binary
            .clone()
    }
    fn acquire(&self, id: &str, approved: bool) -> io::Result<Value> {
        self.acquisition
            .acquire(id, approved, &AtomicBool::new(false))
    }
}

source_test!(
    missing_binary_refuses_launch_without_vendor_network,
    "tests/test_cloak_consent.py::test_missing_binary_refuses_launch_without_vendor_network",
    {
        let fixture = Fixture::new("cloak-consent-missing")?;
        assert!(fixture.acquisition.launch_options().is_err());
        assert!(fixture.vendor.calls().is_empty() && !fixture.binary().exists());
        assert!(
            !fixture.temporary.path().join("vendor-cache").exists(),
            "offer/launch created a vendor cache"
        );
        fixture.temporary.cleanup()?;
        Ok(())
    }
);

source_test!(
    approval_is_bound_to_shown_exact_offer_and_not_persisted,
    "tests/test_cloak_consent.py::test_approval_is_bound_to_shown_exact_offer_and_not_persisted",
    {
        let fixture = Fixture::new("cloak-consent-bound")?;
        let offer = fixture.acquisition.offer();
        assert_eq!(offer["version"], VERSION);
        use sha2::{Digest, Sha256};
        let canonical=br#"{"engine": "cloak", "keyed": false, "platform": "windows-x64", "provider": "CloakHQ/CloakBrowser", "sdk": "0.5.12", "version": "146.0.7680.177.5"}"#;
        let exact: String = Sha256::digest(canonical)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            offer["offer_id"], exact,
            "offer ID differs from the original sorted JSON contract"
        );
        assert!(fixture.vendor.calls().is_empty());
        assert!(fixture.acquire(&fixture.id(), false).is_err());
        assert!(fixture.acquire(&"0".repeat(64), true).is_err());
        assert!(fixture.vendor.calls().is_empty());
        let result = fixture.acquire(&fixture.id(), true)?;
        assert_eq!(result["installed"], true);
        assert!(fixture.binary().is_file());
        assert_eq!(
            fixture.vendor.calls(),
            vec![json!({"browser_version":VERSION})]
        );
        let launch = fixture.acquisition.launch_options()?;
        assert_eq!(
            launch.options,
            serde_json::from_value::<serde_json::Map<String, Value>>(
                json!({"browser_version":VERSION})
            )?
        );
        assert_eq!(
            launch.environment["CLOAKBROWSER_BINARY_PATH"],
            fixture.binary().to_string_lossy()
        );
        assert_eq!(launch.environment["CLOAKBROWSER_AUTO_UPDATE"], "false");
        assert_eq!(
            fixture.vendor.calls().len(),
            1,
            "cached launch performed acquisition"
        );
        assert!(
            fixture.acquire(&fixture.id(), false).is_err(),
            "approval persisted after acquisition"
        );
        let stale = fixture.id();
        fixture
            .vendor
            .patch(|metadata| metadata.version = "147.0.0.0".into());
        assert!(fixture.acquire(&stale, true).is_err());
        assert_eq!(fixture.vendor.calls().len(), 1);
        Ok(())
    }
);

source_test!(
    explicit_binary_override_is_preserved_without_acquisition,
    "tests/test_cloak_consent.py::test_explicit_binary_override_is_preserved_without_acquisition",
    {
        let fixture = Fixture::new("cloak-consent-override")?;
        let binary = fixture.temporary.path().join("external-browser");
        fs::write(&binary, b"user-owned-binary")?;
        fixture
            .vendor
            .patch(|metadata| metadata.binary_override = Some(binary.clone()));
        let launch = fixture.acquisition.launch_options()?;
        assert!(launch.options.is_empty());
        let offer = fixture.acquisition.offer();
        assert_eq!(offer["installed"], true);
        assert_eq!(offer["available"], false);
        assert!(fixture.acquire(&fixture.id(), true).is_err());
        assert!(fixture.vendor.calls().is_empty());
        assert_eq!(fs::read(&binary)?, b"user-owned-binary");
        fs::remove_file(&binary)?;
        assert!(fixture.acquisition.launch_options().is_err());
        assert_eq!(fixture.acquisition.offer()["available"], false);
        Ok(())
    }
);

source_test!(
    unpinned_keyed_or_custom_mirror_acquisition_is_unavailable,
    "tests/test_cloak_consent.py::test_unpinned_keyed_or_custom_mirror_acquisition_is_unavailable",
    {
        let fixture = Fixture::new("cloak-consent-unpinned")?;
        fixture
            .vendor
            .patch(|metadata| metadata.license_key = Some("fixture-not-a-real-license".into()));
        assert_eq!(fixture.acquisition.offer()["available"], false);
        assert!(fixture.acquire(&"0".repeat(64), true).is_err());
        assert!(fixture.vendor.calls().is_empty() && !fixture.binary().exists());
        fixture.vendor.patch(|metadata| {
            metadata.license_key = None;
            metadata.custom_mirror = true;
        });
        assert_eq!(fixture.acquisition.offer()["available"], false);
        assert!(fixture.vendor.calls().is_empty());
        Ok(())
    }
);

source_test!(
    exact_paid_pin_is_honored_but_free_latest_never_downloads,
    "tests/test_cloak_consent.py::test_exact_paid_pin_is_honored_but_free_latest_never_downloads",
    {
        let fixture = Fixture::new("cloak-consent-paid")?;
        fixture.vendor.patch(|metadata| {
            metadata.license_key = Some("fixture-not-a-real-license".into());
            metadata.requested = true;
        });
        *fixture.vendor.license.lock().expect("license fixture") = Some(LicenseInfo {
            valid: true,
            plan: "free".into(),
        });
        let offer = fixture.acquisition.offer();
        assert_eq!(offer["available"], true);
        assert_eq!(offer["keyed"], true);
        let error = fixture
            .acquire(&fixture.id(), true)
            .expect_err("free latest-only license downloaded an exact pin");
        assert!(error.to_string().contains("exact pinned"));
        assert!(fixture.vendor.calls().is_empty() && !fixture.binary().exists());
        *fixture.vendor.license.lock().expect("license fixture") = Some(LicenseInfo {
            valid: true,
            plan: "solo".into(),
        });
        assert_eq!(fixture.acquire(&fixture.id(), true)?["installed"], true);
        assert_eq!(
            fixture.vendor.calls(),
            vec![json!({"browser_version":VERSION,"license_key":"fixture-not-a-real-license"})]
        );
        // Production0.7.2 has no unkeyed fixed default browser. Local metadata must
        // disclose that without resolving latest or claiming a license key.
        let native = cloak_acquisition::NativeVendor::with_environment(
            std::collections::BTreeMap::from([
                (
                    "CLOAKBROWSER_CACHE_DIR".into(),
                    fixture
                        .temporary
                        .path()
                        .join("native cache")
                        .to_string_lossy()
                        .into_owned(),
                ),
                ("CLOAKBROWSER_VERSION".into(), VERSION.into()),
            ]),
            None,
        );
        let native = Acquisition::new(Arc::new(native));
        assert_eq!(native.offer()["available"], false);
        assert!(!fixture.temporary.path().join("native cache").exists());
        Ok(())
    }
);

source_test!(
    same_cache_reentrant_acquisition_refuses_before_vendor_call,
    "tests/test_cloak_consent.py::test_same_cache_reentrant_acquisition_refuses_before_vendor_call",
    {
        let fixture = Fixture::new("cloak-consent-reentrant")?;
        *fixture.vendor.reentrant.lock().expect("reentrant fixture") =
            Some(Arc::downgrade(&fixture.acquisition));
        assert_eq!(fixture.acquire(&fixture.id(), true)?["installed"], true);
        assert_eq!(fixture.vendor.overlaps.load(Ordering::Acquire), 1);
        assert_eq!(fixture.vendor.calls().len(), 1);
        Ok(())
    }
);

source_test!(
    shared_vendor_cache_refuses_another_process_before_download,
    "tests/test_cloak_consent.py::test_shared_vendor_cache_refuses_another_process_before_download",
    {
        // Re-enter only this source case in a real, independently owned native
        // process. The branch exercises Acquisition, not a stand-in lock function.
        if let Some(root) = std::env::var_os("NUMERA_CLOAK_LEASE_CHILD") {
            let vendor = Arc::new(LocalVendor::new(Path::new(&root)));
            let acquisition = Acquisition::new(vendor.clone());
            let offer = acquisition.offer();
            let error = acquisition
                .acquire(
                    offer["offer_id"].as_str().expect("child offer ID"),
                    true,
                    &AtomicBool::new(false),
                )
                .expect_err("another process bypassed cache lease");
            if error.kind() == io::ErrorKind::WouldBlock && vendor.calls().is_empty() {
                println!("{error}");
                std::process::exit(9);
            }
            std::process::exit(3);
        }
        let fixture = Fixture::new("cloak-consent-process")?;
        let cache = fixture
            .vendor
            .metadata
            .lock()
            .expect("metadata fixture")
            .cache
            .clone();
        let _lease = SessionLease::acquire(&SessionLease::path_for(&cache, "cloak-acquisition")?)?;
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "shared_vendor_cache_refuses_another_process_before_download",
                "--nocapture",
            ])
            .env("NUMERA_CLOAK_LEASE_CHILD", fixture.temporary.path());
        let result = numera_core::ownership::capture::run(
            &mut command,
            numera_core::ownership::capture::Limits {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(5),
                bytes: 65536,
                cleanup: Duration::from_secs(2),
            },
        )?;
        assert_eq!(
            result.exit,
            9,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout)?;
        assert!(text.contains("acquisition") && text.contains("in progress"));
        assert!(fixture.vendor.calls().is_empty() && !fixture.binary().exists());
        Ok(())
    }
);

source_test!(
    failed_acquisition_releases_only_its_cache_lease,
    "tests/test_cloak_consent.py::test_failed_acquisition_releases_only_its_cache_lease",
    {
        let fixture = Fixture::new("cloak-consent-failure")?;
        fixture.vendor.fail.store(true, Ordering::Release);
        let target = fixture.id();
        let error = fixture
            .acquire(&target, true)
            .expect_err("controlled vendor failure was swallowed");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(error.to_string().contains("controlled vendor failure"));
        fixture.vendor.fail.store(false, Ordering::Release);
        assert_eq!(fixture.acquire(&target, true)?["installed"], true);
        assert_eq!(fixture.vendor.calls().len(), 1);
        Ok(())
    }
);

fn cli(acquisition: &Acquisition, argv: &[&str]) -> io::Result<Value> {
    let mut command = numera_core::cli::registry();
    command.build();
    let matches = numera_core::cli::parse(std::iter::once("numera").chain(argv.iter().copied()))
        .map_err(io::Error::other)?;
    let args = numera_core::cli::reflection::values(&command, &matches);
    cloak_acquisition::command(
        acquisition,
        args.as_object()
            .ok_or_else(|| io::Error::other("parsed command object missing"))?,
        &AtomicBool::new(false),
    )?
    .ok_or_else(|| io::Error::other("Cloak CLI did not dispatch"))
}
struct SameService {
    acquisition: Arc<Acquisition>,
    root: PathBuf,
}
impl numera_core::http::acquisition_routes::Service for SameService {
    fn offer(&self) -> io::Result<Value> {
        cli(&self.acquisition, &["engines", "--cloak-offer", "--json"])
    }
    fn acquire(&self, root: &Path, argv: &[String]) -> io::Result<Value> {
        assert_eq!(root, self.root);
        cli(
            &self.acquisition,
            &argv.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }
}
source_test!(
    cli_and_http_use_the_same_exact_offer_and_separate_approval,
    "tests/test_cloak_consent.py::test_cli_and_http_use_the_same_exact_offer_and_separate_approval",
    {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        let fixture = Fixture::new("cloak-consent-http")?;
        let shown = cli(
            &fixture.acquisition,
            &["engines", "--cloak-offer", "--json"],
        )?;
        let app = numera_core::http::acquisition_routes::router_with_service(
            fixture.temporary.path().into(),
            "",
            Arc::new(SameService {
                acquisition: fixture.acquisition.clone(),
                root: fixture.temporary.path().into(),
            }),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
        let response=app.clone().oneshot(Request::get("/api/engines/cloak/acquisition").body(Body::empty())?).await?;
        let read:Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await?)?; assert_eq!(read,shown);
        assert!(fixture.vendor.calls().is_empty());
        for (offer_id,approve,expected) in [(shown["offer_id"].clone(),false,400),(json!("stale"),true,409)] {
            let response=app.clone().oneshot(Request::post("/api/engines/cloak/acquisition").header("content-type","application/json").body(Body::from(serde_json::to_vec(&json!({"offer_id":offer_id,"approve":approve}))?))?).await?;
            assert_eq!(response.status().as_u16(),expected); assert!(fixture.vendor.calls().is_empty());
        }
        let response=app.oneshot(Request::post("/api/engines/cloak/acquisition").header("content-type","application/json").body(Body::from(serde_json::to_vec(&json!({"offer_id":shown["offer_id"],"approve":true}))?))?).await?;
        assert_eq!(response.status(),200);
        let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await?)?;
        assert_eq!(result,json!({"engine":"cloak","installed":true,"version":shown["version"],"offer_id":shown["offer_id"]}));
        Ok::<_,Box<dyn std::error::Error>>(())
    })?;
        assert_eq!(
            fixture.vendor.calls(),
            vec![json!({"browser_version":VERSION})]
        );
        Ok(())
    }
);
