//! Registry controls extend the original driver registry source cases.
use super::*;
use browser::registry::{DriverRegistration, DriverRegistry, OptionsFamily};
use std::{
    collections::BTreeMap,
    io,
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn baseline() -> BTreeMap<String, bool> {
    ["playwright", "patchright", "camoufox", "cloak"]
        .into_iter()
        .map(|name| (name.into(), true))
        .collect()
}
fn descriptor(root: &Path, wire: &str, fail_hello: bool) -> DriverRegistration {
    let root = root.to_owned();
    let wire = wire.to_owned();
    let configured = wire.clone();
    DriverRegistration::native(wire,OptionsFamily::Chromium,Some("fixture native SDK".into()),move|context|{
  let lease=numera_core::storage::locks::SessionLease::path_for(&root,&context.profile.id)?;
  assert!(numera_core::storage::locks::SessionLease::acquire(&lease).is_err(),"factory ran before session lease");
  assert!(matches!(numera_core::ownership::claims::read(&root,&context.profile.id),numera_core::ownership::claims::Claim::Absent),"factory saw a premature browser claim");
  browser_fixture::command(&root,"registered",json!({"metadata":{"backends":{configured.clone():{"installed":true}},"default_addons":[]},"root":root,"profile":context.profile.id,"capture":root.join("registered-open.json"),"fail_hello":fail_hello}))
 },||Ok(true)).expect("native driver descriptor")
}
fn ended(root: &Path, id: &str) {
    let pid: u32 = std::fs::read_to_string(root.join("registered-child"))
        .expect("actual leaf PID")
        .parse()
        .expect("PID");
    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            assert!(
                stat.rsplit_once(") ").is_some_and(|(_, fields)| matches!(
                    fields.split_whitespace().next(),
                    Some("Z" | "X")
                )),
                "registered child survived cleanup"
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    assert!(!numera_core::ownership::identity::alive(pid, None).expect("child liveness"));
    assert!(matches!(
        numera_core::ownership::claims::read(root, id),
        numera_core::ownership::claims::Claim::Absent
    ));
    let _lease = numera_core::storage::locks::SessionLease::acquire(
        &numera_core::storage::locks::SessionLease::path_for(root, id).expect("lease path"),
    )
    .expect("lease released");
}
pub fn registration_and_override() {
    let d = fresh();
    let mut p = profile();
    p.name = json!("current profile");
    numera_core::storage::repo::ProfileRepo::new(d.path().into(), None)
        .save(p.clone(), 1000.0)
        .expect("stored profile");
    let registry = Arc::new(DriverRegistry::default());
    assert_eq!(
        registry.names().expect("names"),
        vec!["camoufox", "cloak", "patchright", "playwright"]
    );
    assert!(
        registry
            .register("custom", descriptor(d.path(), "first-wire", false))
            .expect("register")
            .is_none()
    );
    let snapshot = registry.get("custom", &baseline()).expect("get registered");
    assert_eq!(snapshot.wire_backend(), "first-wire");
    let old = registry
        .register("custom", descriptor(d.path(), "custom-wire", false))
        .expect("override")
        .expect("prior descriptor");
    assert_eq!(old.wire_backend(), "first-wire");
    assert_eq!(
        snapshot.wire_backend(),
        "first-wire",
        "override mutated an existing snapshot"
    );
    let weak = Arc::downgrade(&registry);
    let root = d.path().to_owned();
    let observed = Arc::new(AtomicUsize::new(0));
    let count = observed.clone();
    let registration = DriverRegistration::native(
        "custom-wire",
        OptionsFamily::Chromium,
        None,
        move |context| {
            assert_eq!(context.profile.name, json!("current profile"));
            count.fetch_add(1, Ordering::SeqCst);
            let registry = weak.upgrade().expect("registry alive");
            registry.register("custom", descriptor(&root, "replacement-wire", false))?;
            let configured = descriptor(&root, "custom-wire", false);
            configured.command(context)
        },
        || Ok(true),
    )
    .expect("reentrant registration");
    registry
        .register("custom", registration)
        .expect("reentrant override");
    let mut runtime = support::runtime(d.path());
    runtime.registry = Some(registry.clone());
    let owners = numera_core::ownership::registry::Registry::new(
        d.path().into(),
        "source-browser-host".into(),
        None,
    )
    .expect("session registry");
    assert!(
        owners
            .open_browser(&p.id, "stale request name", |session, mut current| {
                current.engine = "custom".into();
                runtime.open(session, current, true, 1001.0)
            })
            .expect("custom registry launch")
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    let open: Value = serde_json::from_slice(
        &std::fs::read(d.path().join("registered-open.json")).expect("actual open capture"),
    )
    .expect("open JSON");
    assert_eq!(
        open["backend"], "custom-wire",
        "launch did not preserve its resolved snapshot"
    );
    assert_eq!(
        open["options"]["user_data_dir"],
        json!(d.path().join("user-data").join(&p.id))
    );
    assert_eq!(open["options"]["headless"], true);
    assert!(d.path().join("registered-claim-before-open").is_file());
    assert!(
        owners
            .stop(&p.id, Duration::from_secs(2))
            .expect("stop custom tree")
    );
    ended(d.path(), &p.id);
    // A builtin override is resolved through the same actual path, with no global registry mutation.
    let p2 = profile();
    registry
        .register("playwright", descriptor(d.path(), "override-wire", false))
        .expect("builtin override");
    let session =
        numera_core::ownership::session::Session::admit(d.path(), &p2.id, "source-browser-host")
            .expect("second admission");
    let mut adapter = runtime
        .open(session, p2.clone(), true, 1002.0)
        .expect("overridden builtin launch");
    adapter
        .finish(Duration::from_secs(2))
        .expect("overridden builtin close");
    drop(adapter);
    ended(d.path(), &p2.id);
}
pub fn availability() {
    let d = fresh();
    let registry = Arc::new(DriverRegistry::default());
    let weak = Arc::downgrade(&registry);
    let factories = Arc::new(AtomicUsize::new(0));
    let count = factories.clone();
    let registration = DriverRegistration::native(
        "unavailable-wire",
        OptionsFamily::Chromium,
        Some("external native dependency".into()),
        move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(Command::new("must-not-start"))
        },
        move || {
            let registry = weak.upgrade().expect("registry alive");
            assert!(
                registry.names()?.contains(&"unavailable".into()),
                "probe ran before publication"
            );
            Ok(false)
        },
    )
    .expect("unavailable descriptor");
    registry
        .register("unavailable", registration)
        .expect("register");
    let values = registry.available(&baseline()).expect("availability");
    assert_eq!(values.len(), 5);
    assert!(!values["unavailable"]);
    let error = registry
        .get("unavailable", &baseline())
        .err()
        .expect("unavailable get refused");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains("external native dependency"));
    let mut runtime = support::runtime(d.path());
    runtime.registry = Some(registry.clone());
    let mut p = profile();
    p.engine = "unavailable".into();
    let session =
        numera_core::ownership::session::Session::admit(d.path(), &p.id, "source-browser-host")
            .expect("admit");
    assert!(runtime.open(session, p.clone(), true, 1000.0).is_err());
    assert_eq!(factories.load(Ordering::SeqCst), 0);
    assert!(matches!(
        numera_core::ownership::claims::read(d.path(), &p.id),
        numera_core::ownership::claims::Claim::Absent
    ));
    let mut environment = numera_core::native_environment::Environment {
        app: d.path().into(),
        home: d.path().into(),
        cwd: d.path().into(),
        host: "source-browser-host".into(),
        browser: runtime,
    };
    environment.browser.adapter=browser_fixture::executable(&d.path().join("default-worker"),"registered",json!({"metadata":{"backends":{"playwright":{"installed":true},"patchright":{"installed":true},"camoufox":{"installed":false},"cloak":{"installed":false}},"default_addons":[]}})).expect("metadata worker");
    let actual = environment
        .metadata(false)
        .expect("actual runtime metadata overlay");
    assert!(!actual["unavailable"]);
    assert!(actual["playwright"]);
    assert!(!actual["camoufox"]);
}
pub fn unknown() {
    let registry = DriverRegistry::default();
    let error = registry
        .get("not-registered", &baseline())
        .err()
        .expect("unknown driver");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains("Unknown engine"));
    let d = fresh();
    let runtime = support::runtime(d.path());
    let mut p = profile();
    p.engine = "custom".into();
    let session =
        numera_core::ownership::session::Session::admit(d.path(), &p.id, "source-browser-host")
            .expect("admit");
    assert!(runtime.open(session, p.clone(), true, 1000.0).is_err());
    assert!(!d.path().join("registered-child").exists());
    assert!(matches!(
        numera_core::ownership::claims::read(d.path(), &p.id),
        numera_core::ownership::claims::Claim::Absent
    ));
}
pub fn failed_handshake() {
    let d = fresh();
    let p = profile();
    let registry = Arc::new(DriverRegistry::default());
    registry
        .register("custom", descriptor(d.path(), "broken-wire", true))
        .expect("register");
    let mut runtime = support::runtime(d.path());
    runtime.registry = Some(registry);
    let mut current = p.clone();
    current.engine = "custom".into();
    let session =
        numera_core::ownership::session::Session::admit(d.path(), &p.id, "source-browser-host")
            .expect("admit");
    let failure = runtime
        .open(session, current, true, 1000.0)
        .err()
        .expect("bad hello refused");
    assert_eq!(failure.kind, "failed");
    ended(d.path(), &p.id);
}
