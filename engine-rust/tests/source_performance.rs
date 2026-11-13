//! Source performance contracts count actual JSON reads/conversions and inspect
//! native persistence effects. They impose no machine-dependent timing bound.
use numera_core::{
    conversion,
    http::tokens::TokenStore,
    proxy::Proxy,
    storage::{pool::ProxyPool, repo::ProfileRepo},
    vault::{
        self,
        testing::{self, Action, Event},
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
mod common;
macro_rules! original {($name:ident,$id:literal,$body:block)=>{#[test]#[doc=$id]fn $name()$body};}
fn temp() -> common::TempDirectory {
    common::TempDirectory::new("source-performance").expect("owned temporary root")
}
fn repo(root: &Path, password: Option<&str>) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), password.map(str::to_owned))
}
fn pool(root: &Path, password: Option<&str>) -> ProxyPool {
    ProxyPool::new(root.to_owned(), password.map(str::to_owned))
}
fn vaulted_pool(root: &Path) -> (ProxyPool, usize) {
    let n = 8;
    pool(root,None).add_many((0..n).map(|i|json!({"type":"HTTP","host":format!("10.0.0.{}",i+1),"port":(9000+i).to_string(),"user":"u","pass":format!("secret-{i}")})).collect()).expect("pool population");
    repo(root, None)
        .enable_vault("master")
        .expect("vault enable");
    (pool(root, Some("master")), n)
}
fn counted<R>(operation: impl FnOnce() -> R) -> (R, Vec<Event>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let result = testing::with_observer(
        move |event| {
            observed.lock().expect("events").push(event.clone());
            Ok(Action::Continue)
        },
        operation,
    );
    let events = events.lock().expect("events").clone();
    (result, events)
}
fn reads(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| e.operation == "json-read" && e.path.extension().is_some_and(|s| s == "json"))
        .count()
}
fn salt() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("salt randomness");
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn cipher(password: &str, salt: &str, rounds: u32) -> fernet::Fernet {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(salt)
        .expect("salt");
    let mut key =
        pbkdf2::pbkdf2_hmac_array::<pbkdf2::sha2::Sha256, 32>(password.as_bytes(), &bytes, rounds);
    let encoded = base64::engine::general_purpose::URL_SAFE.encode(key);
    key.fill(0);
    fernet::Fernet::new(&encoded).expect("independent legacy/current Fernet")
}
struct Client {
    runtime: tokio::runtime::Runtime,
    app: axum::Router,
}
impl Client {
    fn new(root: &Path) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("HTTP runtime");
        let app = runtime
            .block_on(numera_core::http::application::router(
                root.to_owned(),
                "",
                "",
                None,
                "source-performance-host",
                ["playwright", "patchright", "camoufox", "cloak"]
                    .into_iter()
                    .map(|name| (name.to_owned(), false))
                    .collect(),
            ))
            .expect("native router");
        Self { runtime, app }
    }
    fn request(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        self.runtime.block_on(async {
            let response = self
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(if body.is_null() {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&body).expect("JSON request"))
                        })
                        .expect("request"),
                )
                .await
                .expect("HTTP response");
            let status = response.status().as_u16();
            let bytes = to_bytes(response.into_body(), 1048576)
                .await
                .expect("response bytes");
            (
                status,
                serde_json::from_slice(&bytes).expect("JSON response"),
            )
        })
    }
    fn batch(&self, n: usize) -> Value {
        let (status, result) = self.request(
            "POST",
            "/api/profiles/batch",
            json!({"profiles":(0..n).map(|i|json!({"name":format!("p{i}")})).collect::<Vec<_>>()}),
        );
        assert_eq!(status, 200, "{result}");
        result["ids"].clone()
    }
}
original!(
    reading_the_pool_does_not_reread_the_config_per_entry,
    "tests/test_perf_contracts.py::test_reading_the_pool_does_not_reread_the_config_per_entry",
    {
        let temp = temp();
        let (pool, n) = vaulted_pool(temp.path());
        let (rows, events) = counted(|| pool.list().expect("list"));
        assert_eq!(rows.len(), n);
        assert!(rows.iter().all(|row| {
            row["pass"]
                .as_str()
                .is_some_and(|s| s.starts_with("secret-"))
        }));
        assert!(
            reads(&events) >= 2,
            "actual pool/config reads were observed"
        );
        assert!(
            reads(&events) <= 3,
            "{} JSON reads to list {n} proxies",
            reads(&events)
        );
    }
);
original!(
    resolving_many_ids_costs_one_read_each_at_most,
    "tests/test_perf_contracts.py::test_resolving_many_ids_costs_one_read_each_at_most",
    {
        let temp = temp();
        let (pool, n) = vaulted_pool(temp.path());
        let ids = pool
            .list()
            .expect("pool")
            .iter()
            .map(|p| p["id"].as_str().expect("id").to_owned())
            .collect::<Vec<_>>();
        let (_, events) = counted(|| {
            for id in ids {
                assert_eq!(pool.get(&id).expect("get").expect("proxy")["id"], id);
            }
        });
        assert!(reads(&events) >= n, "each ID lookup read the pool");
        assert!(
            reads(&events) <= n + 2,
            "{} JSON reads for {n} lookups",
            reads(&events)
        );
    }
);
original!(
    a_single_update_reads_the_pool_once,
    "tests/test_perf_contracts.py::test_a_single_update_reads_the_pool_once",
    {
        let temp = temp();
        let (pool, _) = vaulted_pool(temp.path());
        let pid = pool.list().expect("pool")[0]["id"]
            .as_str()
            .expect("id")
            .to_owned();
        let (_, events) = counted(|| {
            pool.update(
                &pid,
                json!({"country":"DE"}).as_object().expect("patch").clone(),
            )
            .expect("update")
        });
        assert!(reads(&events) > 0, "actual pool read was observed");
        assert!(
            reads(&events) <= 2,
            "{} JSON reads for one update",
            reads(&events)
        );
        assert_eq!(
            pool.get(&pid).expect("get").expect("proxy")["country"],
            "DE"
        );
    }
);
original!(
    update_many_writes_once_for_the_whole_sweep,
    "tests/test_perf_contracts.py::test_update_many_writes_once_for_the_whole_sweep",
    {
        let temp = temp();
        let (pool, n) = vaulted_pool(temp.path());
        let patches = pool
            .list()
            .expect("pool")
            .iter()
            .map(|p| {
                (
                    p["id"].as_str().expect("id").to_owned(),
                    json!({"country":"FR"}),
                )
            })
            .collect();
        let (touched, events) = counted(|| pool.update_many(patches).expect("sweep"));
        assert_eq!(touched.len(), n);
        assert!(reads(&events) > 0, "actual pool read was observed");
        assert!(
            reads(&events) <= 2,
            "{} JSON reads to update {n} proxies",
            reads(&events)
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.operation == "after-atomic-write"
                    && e.path.file_name().is_some_and(|p| p == "proxies.json"))
                .count(),
            1
        );
        assert!(
            pool.list()
                .expect("final pool")
                .iter()
                .all(|p| p["country"] == "FR")
        );
    }
);
original!(
    update_many_preserves_the_secrets_it_did_not_touch,
    "tests/test_perf_contracts.py::test_update_many_preserves_the_secrets_it_did_not_touch",
    {
        let temp = temp();
        let (pool, _) = vaulted_pool(temp.path());
        let before = pool
            .list()
            .expect("pool")
            .into_iter()
            .map(|p| (p["id"].as_str().expect("id").to_owned(), p["pass"].clone()))
            .collect::<BTreeMap<_, _>>();
        pool.update_many(
            before
                .keys()
                .take(3)
                .map(|id| (id.clone(), json!({"country":"IT"})))
                .collect(),
        )
        .expect("partial sweep");
        assert_eq!(
            pool.list()
                .expect("final pool")
                .into_iter()
                .map(|p| (p["id"].as_str().expect("id").to_owned(), p["pass"].clone()))
                .collect::<BTreeMap<_, _>>(),
            before
        );
    }
);
original!(
    listing_profiles_reads_each_file_once,
    "tests/test_perf_contracts.py::test_listing_profiles_reads_each_file_once",
    {
        let temp = temp();
        let client = Client::new(temp.path());
        client.batch(6);
        let ((status, rows), events) =
            counted(|| client.request("GET", "/api/profiles", Value::Null));
        assert_eq!(status, 200, "{rows}");
        assert_eq!(rows.as_array().expect("profiles").len(), 6);
        assert!(reads(&events) >= 12, "actual twin reads were observed");
        assert!(
            reads(&events) <= 20,
            "{} JSON reads for six profiles: {:?}",
            reads(&events),
            events
                .iter()
                .filter(|e| e.operation == "json-read")
                .map(|e| &e.path)
                .collect::<Vec<_>>()
        );
    }
);
original!(
    health_converts_each_profile_once,
    "tests/test_perf_contracts.py::test_health_converts_each_profile_once",
    {
        let temp = temp();
        let client = Client::new(temp.path());
        client.batch(5);
        let ((status, health), events) =
            counted(|| client.request("GET", "/api/profiles/health", Value::Null));
        assert_eq!(status, 200, "{health}");
        assert_eq!(
            health["profiles"]
                .as_array()
                .expect("health profiles")
                .len(),
            5
        );
        let converted = events
            .iter()
            .filter(|e| e.operation == "health-profile-conversion")
            .collect::<Vec<_>>();
        assert_eq!(
            converted.len(),
            5,
            "converted {} times for five profiles",
            converted.len()
        );
        assert_eq!(
            converted
                .iter()
                .map(|e| &e.path)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            5
        );
    }
);
original!(
    verifying_a_token_does_not_rewrite_the_file_every_time,
    "tests/test_perf_contracts.py::test_verifying_a_token_does_not_rewrite_the_file_every_time",
    {
        let temp = temp();
        let store = TokenStore::new(temp.path().to_owned());
        let made = store.create("CI", 1000.0).expect("token");
        let token = made["token"].as_str().expect("secret token");
        assert!(store.verify(token, 1001.0).expect("verify").is_some());
        let path = temp.path().join("tokens.json");
        let stamp = fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime");
        let (_, events) = counted(|| {
            for _ in 0..20 {
                assert!(store.verify(token, 1001.001).expect("verify").is_some());
            }
        });
        assert_eq!(
            fs::metadata(&path)
                .expect("metadata")
                .modified()
                .expect("mtime"),
            stamp
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.operation == "after-atomic-write" && e.path == path)
                .count(),
            0
        );
    }
);
original!(
    a_revoked_token_still_stops_working_immediately,
    "tests/test_perf_contracts.py::test_a_revoked_token_still_stops_working_immediately",
    {
        let temp = temp();
        let store = TokenStore::new(temp.path().to_owned());
        let keep = store.create("keep", 1000.0).expect("keep");
        let drop = store.create("drop", 1000.0).expect("drop");
        assert!(
            store
                .verify(drop["token"].as_str().expect("token"), 1001.0)
                .expect("verify")
                .is_some()
        );
        assert!(
            store
                .revoke(drop["id"].as_str().expect("id"), true)
                .expect("revoke")
        );
        assert!(
            store
                .verify(drop["token"].as_str().expect("token"), 1001.001)
                .expect("revoked")
                .is_none()
        );
        assert!(
            store
                .verify(keep["token"].as_str().expect("token"), 1001.001)
                .expect("keep")
                .is_some()
        );
    }
);
original!(
    a_v1_secret_still_opens_after_the_round_count_rose,
    "tests/test_perf_contracts.py::test_a_v1_secret_still_opens_after_the_round_count_rose",
    {
        let salt = salt();
        let legacy = format!("enc:v1:{}", cipher("pw", &salt, 200_000).encrypt(b"legacy"));
        assert!(vault::is_envelope(&legacy));
        assert_eq!(
            vault::decrypt("pw", &salt, &legacy).expect("legacy decrypt"),
            "legacy"
        );
    }
);
original!(
    new_secrets_are_written_at_the_current_cost,
    "tests/test_perf_contracts.py::test_new_secrets_are_written_at_the_current_cost",
    {
        let salt = salt();
        let token = vault::encrypt("pw", &salt, "fresh").expect("encrypt");
        assert!(token.starts_with("enc:v2:"));
        let payload = token.strip_prefix("enc:v2:").expect("current envelope");
        assert_eq!(
            cipher("pw", &salt, 600_000)
                .decrypt(payload)
                .expect("current 600000-round key"),
            b"fresh"
        );
        assert!(
            cipher("pw", &salt, 200_000).decrypt(payload).is_err(),
            "current encryption did not use the legacy cost"
        );
        assert_eq!(
            vault::decrypt("pw", &salt, &token).expect("decrypt"),
            "fresh"
        );
    }
);
original!(
    a_secret_from_a_newer_version_says_so,
    "tests/test_perf_contracts.py::test_a_secret_from_a_newer_version_says_so",
    {
        let error = vault::decrypt("pw", &salt(), "enc:v9:whatever").expect_err("unknown version");
        assert!(error.to_string().contains("newer version"), "{error}");
    }
);
original!(
    a_whole_vault_round_trips_across_the_version_change,
    "tests/test_perf_contracts.py::test_a_whole_vault_round_trips_across_the_version_change",
    {
        let temp = temp();
        let root = temp.path();
        let mut p =
            conversion::to_engine(&json!({"id":"p","name":"p","seed":1}), 1000.0).expect("profile");
        p.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://1.2.3.4:8080","username":"u","password":"SECRET"}),
                false,
            )
            .expect("proxy"),
        );
        repo(root, None).save(p, 1000.0).expect("save");
        repo(root, None).enable_vault("master").expect("vault");
        let raw: Value =
            serde_json::from_slice(&fs::read(root.join("profiles/p.json")).expect("stored bytes"))
                .expect("JSON");
        assert!(
            raw["proxy"]["password"]
                .as_str()
                .expect("ciphertext")
                .starts_with("enc:v2:")
        );
        assert_eq!(
            repo(root, Some("master"))
                .resolve("p", 1001.0)
                .expect("resolve")
                .expect("profile")
                .proxy
                .expect("proxy")
                .to_record()["password"],
            "SECRET"
        );
    }
);
original!(
    deleting_many_profiles_takes_one_request,
    "tests/test_perf_contracts.py::test_deleting_many_profiles_takes_one_request",
    {
        let temp = temp();
        let client = Client::new(temp.path());
        let ids = client.batch(5);
        let selection = ids.as_array().expect("ids")[..3].to_vec();
        let (status, result) =
            client.request("POST", "/api/profiles/delete", json!({"ids":selection}));
        assert_eq!(status, 200, "{result}");
        let mut removed = result["removed"].as_array().expect("removed").clone();
        removed.sort_by_key(Value::to_string);
        let mut selected = selection;
        selected.sort_by_key(Value::to_string);
        assert_eq!(removed, selected);
        assert_eq!(result["failed"], json!([]));
        assert_eq!(
            client
                .request("GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("remaining profiles")
                .len(),
            2
        );
    }
);
original!(
    bulk_delete_reports_ids_that_were_not_there,
    "tests/test_perf_contracts.py::test_bulk_delete_reports_ids_that_were_not_there",
    {
        let temp = temp();
        let client = Client::new(temp.path());
        let (status, made) = client.request("POST", "/api/profiles", json!({"name":"real"}));
        assert_eq!(status, 200, "{made}");
        let (status, result) = client.request(
            "POST",
            "/api/profiles/delete",
            json!({"ids":[made["id"],"deadbeef1234"]}),
        );
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["removed"], json!([made["id"]]));
        assert_eq!(result["missing"], json!(["deadbeef1234"]));
    }
);
original!(
    a_malformed_bulk_delete_is_a_4xx,
    "tests/test_perf_contracts.py::test_a_malformed_bulk_delete_is_a_4xx",
    {
        let temp = temp();
        let client = Client::new(temp.path());
        for body in [json!({"ids":"nope"}), json!({"ids":[1,2]}), json!({})] {
            let (status, result) = client.request("POST", "/api/profiles/delete", body);
            assert!([400, 422].contains(&status), "{result}");
        }
        drop(client);
        temp.cleanup().expect("owned root cleanup");
    }
);
