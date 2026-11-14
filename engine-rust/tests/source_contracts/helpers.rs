use super::*;
pub(super) fn refused_field(door: &str, field: &str, value: Value) {
    let tmp = common::TempDirectory::new("contract-refusal").expect("root");
    let root = tmp.path();
    let mut patch = json!({});
    patch[field] = value;
    let mut method = "POST";
    let mut uri = "/api/profiles".to_owned();
    let body = match door {
        "create" => {
            let mut p = json!({"name":"poisoned"});
            p.as_object_mut()
                .expect("object")
                .extend(patch.as_object().expect("patch").clone());
            p
        }
        "update" => {
            let p = made(root, json!({"name":"clean"}));
            method = "PUT";
            uri = format!("/api/profiles/{}", p["id"].as_str().expect("ID"));
            patch
        }
        "batch" => {
            uri = "/api/profiles/batch".into();
            let mut bad = json!({"name":"second"});
            bad.as_object_mut()
                .expect("object")
                .extend(patch.as_object().expect("patch").clone());
            let mut b = json!({"profiles":[{"name":"first"},{},{"name":"third"}]});
            b["profiles"][1] = bad;
            b
        }
        "bulk" => {
            uri = "/api/profiles/bulk".into();
            let ids = ["a", "b", "c"].map(|name| made(root, json!({"name":name}))["id"].clone());
            let mut b = json!({"ids":ids});
            b["patch"] = patch;
            b
        }
        _ => panic!("unknown door"),
    };
    let before = snapshot(root);
    let (s, v) = http(root, method, &uri, body);
    assert_refused(s, &v, field);
    assert_eq!(snapshot(root), before, "refusal changed payload bytes");
}
pub(super) fn refused_record(path: &str, value: Value, fragment: &str) {
    let tmp = common::TempDirectory::new("contract-record").expect("root");
    let mut p = record();
    let parts = path.split('.').collect::<Vec<_>>();
    if parts.len() == 2 {
        p[parts[0]][parts[1]] = value;
    } else {
        p[path] = value;
    }
    let before = p.clone();
    let errors = problems(&p, Some(write_contract::ENGINE_RECORD));
    assert!(
        errors
            .iter()
            .any(|e| e.contains(path) && e.contains(fragment)),
        "{path} should contain {fragment}: {errors:?}"
    );
    assert_eq!(p, before, "validation mutated input");
    let before = snapshot(tmp.path());
    if let Ok(model) = Profile::from_record(&p) {
        let modeled = serde_json::to_value(&model).expect("modeled record");
        let retained = if parts.len() == 2 {
            modeled[parts[0]][parts[1]] == p[parts[0]][parts[1]]
        } else {
            modeled[path] == p[path]
        };
        if retained {
            let error = repo(tmp.path())
                .save(model, 1001.0)
                .err()
                .expect("invalid engine object");
            assert!(
                error.to_string().contains(path) && error.to_string().contains(fragment),
                "{error}"
            );
            assert_eq!(snapshot(tmp.path()), before);
        }
    }
    assert!(
        import_file(tmp.path(), p).is_err(),
        "raw invalid complete record was written"
    );
    assert_eq!(snapshot(tmp.path()), before);
}
pub(super) fn accepted_record(path: &str, value: Value) {
    let tmp = common::TempDirectory::new("contract-record-control").expect("root");
    let mut p = record();
    p["fingerprint"][path] = value.clone();
    assert!(problems(&p, None).is_empty(), "valid control refused");
    let p = Profile::from_record(&p).expect("model legitimate edge");
    let id = p.id.clone();
    repo(tmp.path()).save(p, 1001.0).expect("save edge");
    let actual = read(tmp.path(), "profiles", &id);
    assert_eq!(actual["fingerprint"][path], value);
}
pub(super) fn vendor_refused(value: Value, field: &str, index: Option<usize>) {
    let tmp = common::TempDirectory::new("contract-vendor").expect("root");
    let rows = if index.is_some() {
        let mut r = json!([{"name":"first"},{"name":"second"},{},{"name":"fourth"}]);
        r[2] = value;
        r
    } else {
        Value::Array(vec![value])
    };
    let before = snapshot(tmp.path());
    let (s, v) = http(tmp.path(), "POST", "/api/profiles/import", importing(rows));
    assert_refused(s, &v, field);
    if let Some(index) = index {
        assert!(v.to_string().contains(&format!("profiles[{index}]")), "{v}");
    }
    assert_eq!(snapshot(tmp.path()), before);
}
pub(super) fn vendor_alias(value: Value, field: &str, want: Value) {
    let tmp = common::TempDirectory::new("contract-vendor-alias").expect("root");
    let (s, v) = http(
        tmp.path(),
        "POST",
        "/api/profiles/import",
        importing(Value::Array(vec![value])),
    );
    assert_eq!(s, 200, "{v}");
    valid_twin(tmp.path());
    let id = ids(tmp.path(), "dashboard")
        .into_iter()
        .next()
        .expect("imported ID");
    let got = read(tmp.path(), "dashboard", &id);
    assert_eq!(got[field], want);
}
pub(super) fn cli_refused(value: Value, fragment: &str) {
    let tmp = common::TempDirectory::new("contract-cli-refused").expect("root");
    let before = snapshot(tmp.path());
    let out = cli_import(tmp.path(), value);
    assert_eq!(out.exit, 1, "{}", out.payload);
    assert!(
        out.payload.to_string().contains(fragment),
        "{}",
        out.payload
    );
    assert_eq!(snapshot(tmp.path()), before);
}
pub(super) fn good_vendor() {
    let tmp = common::TempDirectory::new("contract-cli-good").expect("root");
    let out = cli_import(
        tmp.path(),
        json!([{"name":"from-gologin","os":"win","userAgent":"Mozilla/5.0","resolution":"1920x1080"}]),
    );
    assert_eq!(out.exit, 0, "{}", out.payload);
    assert_eq!(
        repo(tmp.path())
            .list(1001.0)
            .expect("list")
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        [json!("from-gologin")]
    );
    valid_twin(tmp.path());
}
pub(super) fn proxy_intent() {
    let px = Proxy::from_record(
        &json!({"server":"http://h:1","username":"u","password":"stored","country":"DE"}),
        true,
    )
    .expect("stored proxy");
    assert_eq!(px.password_intent(), "preserve");
    let mut p = profile("p");
    p.proxy = Some(px);
    assert!(
        write_contract::profile_problems(Input::Profile(&p), "profile", None)
            .expect("validate")
            .is_empty()
    );
    assert_eq!(
        p.proxy.as_ref().expect("proxy").password_intent(),
        "preserve"
    );
}
pub(super) fn ordinary_proxy() {
    let tmp = common::TempDirectory::new("contract-proxy").expect("root");
    let mut p = profile("proxy");
    p.proxy = Some(
        Proxy::from_record(
            &json!({"server":"socks5://h:1080","username":"u","password":"p","country":"NL"}),
            false,
        )
        .expect("proxy"),
    );
    let p = repo(tmp.path()).save(p, 1001.0).expect("save proxy");
    assert_eq!(p.proxy.expect("proxy").server, "socks5://h:1080");
    assert_eq!(
        repo(tmp.path()).list(1001.0).expect("list")[0]
            .proxy
            .as_ref()
            .expect("proxy")
            .username
            .as_deref(),
        Some("u")
    );
}
pub(super) fn legacy(root: &Path) {
    fs::create_dir_all(root.join("dashboard")).expect("dashboard");
    for (id, extra) in [
        ("nameless", json!({})),
        ("numeric", json!({"name":123})),
        ("nulled", json!({"name":null})),
    ] {
        let mut rec = json!({"id":id,"_rev":1});
        rec.as_object_mut()
            .expect("record")
            .extend(extra.as_object().expect("extra").clone());
        fs::write(
            root.join("dashboard").join(format!("{id}.json")),
            encode(&rec),
        )
        .expect("legacy bytes");
    }
}
pub(super) fn assert_listing_preserves_original_files(
    root: &Path,
    before: &BTreeMap<PathBuf, Vec<u8>>,
    reconciles: bool,
) {
    let mut after = snapshot(root);
    // The source engine's union listing materializes dashboard-only rows.
    // Read repair must preserve every existing byte; an allowed engine twin
    // is a distinct source behavior, not a rewrite of the damaged evidence.
    let mut twins = BTreeSet::new();
    if reconciles {
        for path in before
            .keys()
            .filter(|path| path.parent() == Some(Path::new("dashboard")))
        {
            let twin = Path::new("profiles").join(path.file_name().expect("dashboard filename"));
            if !before.contains_key(&twin) {
                twins.insert(twin);
            }
        }
    }
    for twin in twins {
        let bytes = after
            .remove(&twin)
            .expect("source union listing created its engine twin");
        let profile =
            Profile::from_record(&serde_json::from_slice::<Value>(&bytes).expect("engine JSON"))
                .expect("readable engine twin");
        assert_eq!(
            twin.file_stem().and_then(|name| name.to_str()),
            Some(profile.id.as_str())
        );
    }
    assert_eq!(
        &after, before,
        "listing changed preexisting bytes or created an unexpected file"
    );
}
pub(super) fn legacy_view(route: &str) {
    let tmp = common::TempDirectory::new("contract-legacy").expect("root");
    legacy(tmp.path());
    made(tmp.path(), json!({"name":"healthy"}));
    let before = snapshot(tmp.path());
    let (s, body) = http(tmp.path(), "GET", route, Value::Null);
    assert_eq!(s, 200, "{body}");
    let rows = if route.ends_with("health") {
        body["profiles"].as_array()
    } else if route.ends_with("collisions") {
        assert_eq!(body["total"], 4);
        None
    } else {
        body.as_array()
    };
    if let Some(rows) = rows {
        assert_eq!(rows.len(), 4);
        for row in rows {
            assert!(row["name"].is_string(), "{row}");
            if row["id"] == "numeric" {
                assert_eq!(row["name"], "123");
            }
        }
    }
    assert_listing_preserves_original_files(tmp.path(), &before, route == "/api/profiles");
}
pub(super) fn revision(stale: bool) {
    let tmp = common::TempDirectory::new("contract-revision").expect("root");
    let original = made(tmp.path(), json!({"name":"a"}));
    let (s, mut fresh) = put(tmp.path(), &original["id"], json!({"notes":"fresh"}));
    assert_eq!(s, 200, "{fresh}");
    let mut update = if stale {
        original.clone()
    } else {
        fresh.clone()
    };
    update["name"] = "renamed".into();
    let (s, v) = put(tmp.path(), &original["id"], update);
    if stale {
        assert_eq!(s, 409, "{v}");
        assert_eq!(v["kind"], "StaleProfile");
        fresh = read(
            tmp.path(),
            "dashboard",
            original["id"].as_str().expect("id"),
        );
        assert_eq!(fresh["notes"], "fresh");
    } else {
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["name"], "renamed");
        assert_eq!(v["notes"], "fresh");
    }
}
pub(super) fn patch_fields() {
    let tmp = common::TempDirectory::new("contract-patches").expect("root");
    let p = made(tmp.path(), json!({"name":"p","notes":"keep"}));
    assert_eq!(put(tmp.path(), &p["id"], json!({"notes":"from-A"})).0, 200);
    assert_eq!(put(tmp.path(), &p["id"], json!({"tags":["from-B"]})).0, 200);
    let p = read(tmp.path(), "dashboard", p["id"].as_str().expect("ID"));
    assert_eq!(p["notes"], "from-A");
    assert_eq!(p["tags"], json!(["from-B"]));
    assert_eq!(p["_rev"], 3);
}
pub(super) fn password_edit(mode: &str) {
    let tmp = common::TempDirectory::new("contract-secret").expect("root");
    let p = made(
        tmp.path(),
        json!({"name":"p","proxy":{"type":"HTTP","host":"h","port":"1","user":"u","pass":"secret"}}),
    );
    let payload = match mode {
        "preserve" => json!({"notes":"unrelated"}),
        "clear" => json!({"proxy":{"type":"HTTP","host":"h","port":"1","pass":""}}),
        "replace" => json!({"proxy":{"type":"HTTP","host":"h","port":"1","pass":"new"}}),
        "remove" => json!({"proxy":null}),
        _ => panic!("secret mode"),
    };
    let (s, v) = put(tmp.path(), &p["id"], payload);
    assert_eq!(s, 200, "{v}");
    let (s, listed) = http(tmp.path(), "GET", "/api/profiles", Value::Null);
    assert_eq!(s, 200);
    if mode == "remove" {
        assert!(listed[0]["proxy"].is_null());
    } else {
        assert_eq!(
            listed[0]["proxy"]["pass"],
            match mode {
                "preserve" => "secret",
                "clear" => "",
                "replace" => "new",
                _ => panic!("mode"),
            }
        );
    }
}
pub(super) fn vendor_unreadable(value: Value) {
    let tmp = common::TempDirectory::new("contract-unreadable").expect("root");
    let (s, v) = http(
        tmp.path(),
        "POST",
        "/api/profiles/import",
        importing(Value::Array(vec![value])),
    );
    assert!(s < 500, "{v}");
    if s < 400 {
        assert!(
            v["results"][0]["issues"]
                .as_array()
                .is_some_and(|v| !v.is_empty()),
            "silent importer repair: {v}"
        );
        for id in ids(tmp.path(), "dashboard") {
            assert!(read(tmp.path(), "dashboard", &id)["tags"].is_array());
        }
    } else {
        assert!(ids(tmp.path(), "dashboard").is_empty());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
}
pub(super) fn corrupt(root: &Path) -> String {
    let p = made(root, json!({"name":"legacy"}));
    let id = p["id"].as_str().expect("ID").to_owned();
    let mut rec = read(root, "dashboard", &id);
    rec["fontList"] = json!(17);
    rec["tags"] = json!([{}, "work"]);
    rec["cores"] = number("Infinity");
    fs::write(
        root.join("dashboard").join(format!("{id}.json")),
        encode(&rec),
    )
    .expect("corrupt legacy bytes");
    id
}
pub(super) fn corrupt_view(route: &str) {
    let tmp = common::TempDirectory::new("contract-corrupt-view").expect("root");
    corrupt(tmp.path());
    made(tmp.path(), json!({"name":"innocent"}));
    let (s, v) = http(tmp.path(), "GET", route, Value::Null);
    assert!(s < 500, "{v}");
    let n = if route.ends_with("health") {
        v["profiles"].as_array().expect("health rows").len()
    } else if route.ends_with("collisions") {
        v["total"].as_u64().expect("total") as usize
    } else {
        v.as_array().expect("profiles").len()
    };
    assert_eq!(n, 2, "corrupt record was hidden");
}
pub(super) fn proxy_roundtrip(entry: Value) {
    let tmp = common::TempDirectory::new("contract-structured-proxy").expect("root");
    let (s, v) = http(
        tmp.path(),
        "POST",
        "/api/proxies",
        json!({"proxies":[entry.clone()]}),
    );
    assert_eq!(s, 200, "{v}");
    let (s, rows) = http(tmp.path(), "GET", "/api/proxies", Value::Null);
    assert_eq!(s, 200);
    let got = rows
        .as_array()
        .expect("proxies")
        .last()
        .expect("stored proxy");
    assert_eq!(got["host"], entry["host"]);
    let port = entry["port"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| entry["port"].to_string());
    assert_eq!(got["port"], port);
    assert_eq!(
        got["type"],
        entry.get("type").cloned().unwrap_or(json!("HTTP"))
    );
}
pub(super) fn conversion_failure(import: bool) {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-conversion-fault").expect("root");
    let before = snapshot(tmp.path());
    with_observer(
        |event| {
            if event.operation == "before-dashboard-conversion" {
                return Err(std::io::Error::other("unconvertible"));
            }
            Ok(Action::Continue)
        },
        || {
            if import {
                let (s, v) = http(
                    tmp.path(),
                    "POST",
                    "/api/profiles/import",
                    importing(
                        json!([{"name":"vendor-one","os":"Windows"},{"name":"vendor-two","os":"Windows"}]),
                    ),
                );
                assert!((400..500).contains(&s), "{v}");
            } else {
                assert!(
                    dash(tmp.path())
                        .save(
                            json!({"name":"odd"}),
                            serde_json::Map::new(),
                            None,
                            None,
                            1001.0
                        )
                        .is_err()
                );
            }
        },
    );
    assert_eq!(
        snapshot(tmp.path()),
        before,
        "failed conversion lost engine twin"
    );
}
pub(super) fn full_roundtrip() {
    let tmp = common::TempDirectory::new("contract-full").expect("root");
    let full = json!({"name":"full","folder":"Ads","os":"macOS","engine":"camoufox","browser":"Chrome","browserVersion":"131.0.0.0","userAgent":"Mozilla/5.0 (Macintosh)","screen":"2560x1440","cores":12,"memory":32,"webglVendor":"Apple","webglRenderer":"Apple M2","canvas":"Noise","webrtc":"Altered","audio":"Noise","fonts":"Masked","clientRects":"Noise","webglImage":"Noise","mediaDevices":"Masked","locale":"en-GB","timezone":"Europe/London","geo":"Prompt","mediaVideo":2,"mediaAudio":2,"mediaSpeakers":1,"fontList":"Arial\nHelvetica","osVersion":"14.0.0","deviceModel":"","maxTouchPoints":0,"devicePixelRatio":2.0,"seed":4242,"dnt":true,"proxy":{"type":"HTTP","host":"127.0.0.1","port":"8080","user":"u","pass":"p","country":"GB"},"notes":"hello","tags":["work","ads"],"startupUrls":"https://example.test/","extensions":"","launchArgs":"--mute-audio","bookmarks":"Home | https://example.test/","dns":"1.1.1.1","ipBased":["timezone"],"clearOnStart":["cookies"],"switchedOff":["googleServices"]});
    let got = made(tmp.path(), full.clone());
    for key in [
        "name",
        "folder",
        "os",
        "engine",
        "screen",
        "cores",
        "seed",
        "tags",
        "dns",
        "ipBased",
        "clearOnStart",
        "switchedOff",
    ] {
        assert_eq!(got[key], full[key], "field {key}");
    }
    valid_twin(tmp.path());
}
pub(super) fn bad_later_complete(index: usize, changes: bool, field: &str) {
    let tmp = common::TempDirectory::new("contract-batch-complete").expect("root");
    repo(tmp.path())
        .save(profile("already-here"), 1001.0)
        .expect("seed");
    let before = snapshot(tmp.path());
    let context = if changes {
        "profile".to_owned()
    } else {
        format!("profiles[{index}]")
    };
    let mut raw = record();
    raw["fingerprint"][field] = match field {
        "seed" => Value::Null,
        "timezone" => json!([]),
        "user_agent" => json!(123),
        "device_memory" => json!(-5),
        _ => panic!("unknown original malformed batch field"),
    };
    let error = write_contract::require(
        Input::Json(&raw),
        &context,
        Some(write_contract::ENGINE_RECORD),
    )
    .expect_err("original raw field refused")
    .to_string();
    assert!(error.contains(field) && error.contains(&context), "{error}");
    assert!(import_file(tmp.path(), raw).is_err());
    assert_eq!(snapshot(tmp.path()), before);
    let mut rows = (0..if changes { 1 } else { index.max(2) + 1 })
        .map(|i| profile(&format!("p{i}")))
        .collect::<Vec<_>>();
    rows[index].fingerprint.device_memory = json!(-5);
    let result = repo(tmp.path()).save_many(
        rows,
        if changes {
            serde_json::Map::from_iter([("default_engine".into(), json!("camoufox"))])
        } else {
            serde_json::Map::new()
        },
        "reject",
        1002.0,
    );
    let e = result.err().expect("invalid batch refused").to_string();
    assert!(e.contains(&context), "{e}");
    assert_eq!(snapshot(tmp.path()), before);
}
pub(super) fn tag_operands(patch: Value, want: Value) {
    let tmp = common::TempDirectory::new("contract-tags-operation").expect("root");
    let ids =
        ["a", "b"].map(|name| made(tmp.path(), json!({"name":name,"tags":["old"]}))["id"].clone());
    let (s, v) = http(
        tmp.path(),
        "POST",
        "/api/profiles/bulk",
        json!({"ids":ids,"patch":patch}),
    );
    assert_eq!(s, 200, "{v}");
    for id in ids {
        assert_eq!(
            read(tmp.path(), "dashboard", id.as_str().expect("ID"))["tags"],
            want
        );
    }
}
pub(super) fn write_fault(kind: &str, nth: usize) {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-write-fault").expect("root");
    let root = tmp.path();
    let mut target = Vec::new();
    if kind != "batch" {
        for name in if kind == "bulk" {
            vec!["a", "b", "c"]
        } else {
            vec!["first"]
        } {
            target.push(made(root, json!({"name":name}))["id"].clone());
        }
    }
    let before = snapshot(root);
    let mut count = 0;
    with_observer(
        move |event| {
            if event.operation == "before-batch-write" {
                count += 1;
                if count == nth {
                    return Err(std::io::Error::other("no space left on device"));
                }
            }
            Ok(Action::Continue)
        },
        || {
            let (uri, body) = match kind {
                "bulk" => (
                    "/api/profiles/bulk",
                    json!({"ids":target,"patch":{"engine":"camoufox","notes":"x"}}),
                ),
                "batch" => (
                    "/api/profiles/batch",
                    json!({"profiles":[{"name":"a","engine":"camoufox"},{"name":"b","engine":"camoufox"}]}),
                ),
                _ => (
                    "/api/profiles",
                    json!({"name":"second","engine":"camoufox"}),
                ),
            };
            let (s, v) = http(root, "POST", uri, body);
            assert_eq!(s, 500, "injected real write fault not reached: {v}");
        },
    );
    assert_eq!(
        snapshot(root),
        before,
        "failed request retained a payload mutation"
    );
}
pub(super) fn folder_change(mode: &str) {
    use numera_core::storage::folders::Folders;
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-folder-change").expect("root");
    let root = tmp.path();
    let target = ["a", "b", "c"].map(|name| {
        made(root, json!({"name":name,"folder":"work"}))["id"]
            .as_str()
            .expect("id")
            .to_owned()
    });
    let folders = Folders::new(root.to_owned());
    folders.create("work").expect("folder");
    let before = snapshot(root);
    if mode == "fail" {
        let mut count = 0;
        with_observer(
            move |event| {
                if event.operation == "before-batch-write" {
                    count += 1;
                    if count == 3 {
                        return Err(std::io::Error::other("disk full"));
                    }
                }
                Ok(Action::Continue)
            },
            || {
                assert!(folders.rename("work", "client-a").is_err());
            },
        );
        assert_eq!(snapshot(root), before);
    } else {
        if mode == "delete" {
            folders.delete("work", Some("kept")).expect("delete folder");
        } else {
            folders.rename("work", "client-a").expect("rename folder");
            assert!(
                folders
                    .listing()
                    .expect("folders")
                    .contains(&"client-a".into())
            );
            assert!(!folders.listing().expect("folders").contains(&"work".into()));
        }
        for id in target {
            assert_eq!(
                read(root, "dashboard", &id)["folder"],
                if mode == "delete" { "kept" } else { "client-a" }
            );
        }
    }
}
pub(super) fn delete_fault(stage: &str) {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-delete-fault").expect("root");
    let root = tmp.path();
    let p = made(root, json!({"name":"both"}));
    let id = p["id"].as_str().expect("id");
    let session = repo(root).user_data_path(id).expect("session");
    fs::write(session.join("Cookies"), "real").expect("session bytes");
    let before = snapshot(root);
    let stage = stage.to_owned();
    with_observer(
        move |event| {
            if (stage == "engine"
                && event.operation == "before-delete-metadata"
                && event.path.parent().is_some_and(|p| p.ends_with("profiles")))
                || (stage == "detach" && event.operation == "before-session-detach")
            {
                return Err(std::io::Error::other("in use"));
            }
            Ok(Action::Continue)
        },
        || {
            assert!(
                dash(root)
                    .delete(id, "source-contract-host", 1001.0)
                    .is_err()
            );
        },
    );
    assert_eq!(snapshot(root), before, "delete failure lost originals");
    assert_eq!(
        fs::read_to_string(session.join("Cookies")).expect("session restored"),
        "real"
    );
}
pub(super) fn delete_order() {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-delete-order").expect("root");
    let root = tmp.path();
    let p = made(root, json!({"name":"with-session"}));
    let id = p["id"].as_str().expect("id");
    let live = repo(root).user_data_path(id).expect("session");
    fs::write(live.join("Cookies"), "real").expect("bytes");
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = order.clone();
    with_observer(
        move |event| {
            if [
                "before-session-detach",
                "before-delete-metadata",
                "before-session-reclaim",
            ]
            .contains(&event.operation)
            {
                observed.lock().expect("order").push(event.operation);
            }
            Ok(Action::Continue)
        },
        || {
            assert!(
                dash(root)
                    .delete(id, "source-contract-host", 1001.0)
                    .expect("delete")
            );
        },
    );
    let order = order.lock().expect("order");
    assert_eq!(order.first(), Some(&"before-session-detach"));
    assert_eq!(order.last(), Some(&"before-session-reclaim"));
    assert!(order.contains(&"before-delete-metadata"));
    assert!(!live.exists());
    assert!(ids(root, "profiles").is_empty());
    assert!(ids(root, "dashboard").is_empty());
}
pub(super) fn batch_controls(mode: &str) {
    let tmp = common::TempDirectory::new("contract-batch-control").expect("root");
    let root = tmp.path();
    let (s, v) = http(
        root,
        "POST",
        "/api/profiles/batch",
        json!({"profiles":[{"name":"a"},{"name":"b"}]}),
    );
    assert_eq!(s, 200, "{v}");
    let ids = v["ids"].as_array().expect("IDs");
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    let rows = ids
        .iter()
        .map(|id| read(root, "dashboard", id.as_str().expect("id")))
        .collect::<Vec<_>>();
    match mode {
        "identity" => {
            assert!(
                rows.iter()
                    .all(|r| r["seed"].as_i64().is_some_and(|v| v != 0))
            );
            assert_ne!(rows[0]["seed"], rows[1]["seed"]);
        }
        "listing" => {
            let (s, listed) = http(root, "GET", "/api/profiles", Value::Null);
            assert_eq!(s, 200);
            for row in listed.as_array().expect("listing") {
                assert!(row["tags"].is_array());
                assert!(["running", "stopped"].contains(&row["status"].as_str().expect("status")));
            }
        }
        _ => {
            for row in rows {
                assert!(row["tags"].is_array());
                assert!(row["startupUrls"].is_string());
                assert!(!row["userAgent"].as_str().expect("UA").is_empty());
                assert!(row["os"].is_string());
            }
        }
    }
    valid_twin(root);
}
pub(super) fn rollback_failures() {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-rollback-failures").expect("root");
    let first = tmp.path().join("one.json");
    let second = tmp.path().join("two.json");
    atomic::write_json(&first, &json!({"keep":"original"})).expect("original");
    let mut batch = Batch::default();
    batch
        .write(&first, &json!({"keep":"changed"}))
        .expect("first write");
    batch
        .write(&second, &json!({"any":"thing"}))
        .expect("second write");
    let fail = first.clone();
    let error = with_observer(
        move |event| {
            if event.operation == "before-restore" && event.path == fail {
                return Err(std::io::Error::other("the volume is read-only"));
            }
            Ok(Action::Continue)
        },
        || batch.fail(std::io::Error::other("no space left on device")),
    );
    let rollback = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<atomic::RollbackFailed>())
        .expect("typed rollback failure");
    assert_eq!(rollback.cause.kind(), std::io::ErrorKind::Other);
    assert!(rollback.cause.to_string().contains("no space left"));
    assert!(std::error::Error::source(rollback).is_some());
    assert!(error.to_string().contains("no space left"));
    assert!(error.to_string().contains(first.to_string_lossy().as_ref()));
    let failures = &rollback.failures;
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, first);
    assert!(failures[0].1.to_string().contains("read-only"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(first).expect("changed bytes")).expect("JSON"),
        json!({"keep":"changed"})
    );
    assert!(
        !second.exists(),
        "rollback failed to continue after restoration fault"
    );
}
pub(super) fn cli_bad_schedule() {
    let tmp = common::TempDirectory::new("contract-cli-schedule").expect("root");
    let repository = repo(tmp.path());
    let p = repository.save(profile("p"), 1000.0).expect("seed profile");
    let installed = BTreeMap::new();
    let settings = numera_core::cli::settings_commands::Settings {
        root: tmp.path(),
        repo: &repository,
        installed: &installed,
        environment_token: false,
    };
    let mut args = json!({"ref":p.id,"every":6,"minutes":0});
    args["minutes"] = number("NaN");
    let result = settings.schedule("add", args.as_object().expect("arguments"), 1001.0);
    assert!(
        result.is_err() || result.is_ok_and(|out| out.exit != 0),
        "CLI wrote nonfinite schedule"
    );
    assert!(!tmp.path().join("schedules.json").exists());
}
pub(super) fn folder_rollback_failure() {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-folder-rollback").expect("root");
    for name in ["a", "b"] {
        made(tmp.path(), json!({"name":name,"folder":"Old"}));
    }
    let mut count = 0;
    let result = with_observer(
        move |event| {
            if event.operation == "before-batch-write" {
                count += 1;
                if count >= 2 {
                    return Err(std::io::Error::other("no space left on device"));
                }
            }
            if event.operation == "before-restore" {
                return Err(std::io::Error::other("the volume is read-only"));
            }
            Ok(Action::Continue)
        },
        || numera_core::storage::folders::Folders::new(tmp.path().to_owned()).rename("Old", "New"),
    );
    let error = result.expect_err("folder rollback failed");
    let rollback = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<atomic::RollbackFailed>())
        .expect("typed folder rollback failure");
    assert!(!rollback.failures.is_empty());
    assert!(rollback.cause.to_string().contains("no space left"));
    for (path, _) in &rollback.failures {
        assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    }
}
pub(super) fn damaged(root: &Path) {
    fs::create_dir_all(root.join("dashboard")).expect("dashboard");
    for mut p in [
        json!({"id":"aaaaaaaaaaaa","name":123}),
        json!({"id":"bbbbbbbbbbbb","name":null}),
        json!({"id":"cccccccccccc"}),
        json!({"id":"dddddddddddd","name":["a"],"tags":[{},"keep"]}),
        json!({"id":"eeeeeeeeeeee","name":"fine"}),
    ] {
        if p["id"] == "eeeeeeeeeeee" {
            p["cores"] = number("Infinity");
        }
        fs::write(
            root.join("dashboard")
                .join(format!("{}.json", p["id"].as_str().expect("id"))),
            encode(&p),
        )
        .expect("damaged record");
    }
}
pub(super) fn damaged_view(route: &str) {
    let tmp = common::TempDirectory::new("contract-damaged-views").expect("root");
    damaged(tmp.path());
    made(tmp.path(), json!({"name":"healthy"}));
    let before = snapshot(tmp.path());
    let (s, v) = http(tmp.path(), "GET", route, Value::Null);
    assert_eq!(s, 200, "{v}");
    if let Some(rows) = if route.ends_with("health") {
        v["profiles"].as_array()
    } else {
        v.as_array()
    } {
        assert_eq!(rows.len(), 6, "damaged profile was hidden");
        assert!(rows.iter().any(|r| r["name"] == "healthy"));
        for row in rows {
            assert!(row["name"].is_string());
            if let Some(tags) = row["tags"].as_array() {
                assert!(tags.iter().all(Value::is_string));
            }
        }
    } else {
        assert_eq!(v["total"], 6);
    }
    assert_listing_preserves_original_files(tmp.path(), &before, route == "/api/profiles");
}
pub(super) fn revision_disk(mode: &str) {
    let tmp = common::TempDirectory::new("contract-revision-disk").expect("root");
    let root = tmp.path();
    if mode == "import" {
        let (s, v) = http(
            root,
            "POST",
            "/api/profiles/import",
            importing(
                json!({"profiles":[{"name":"imported","engine":"playwright","fingerprint":{"os":"windows"}}]}),
            ),
        );
        assert_eq!(s, 200, "{v}");
    } else {
        let (s, v) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"a"},{"name":"b"}]}),
        );
        assert_eq!(s, 200, "{v}");
    }
    for expected in 1..=if mode == "bulk" { 3 } else { 1 } {
        if expected > 1 {
            let all = ids(root, "dashboard").into_iter().collect::<Vec<_>>();
            let (s, v) = http(
                root,
                "POST",
                "/api/profiles/bulk",
                json!({"ids":all,"patch":{"notes":format!("revision-{expected}")}}),
            );
            assert_eq!(s, 200, "{v}");
        }
        let (s, served) = http(root, "GET", "/api/profiles", Value::Null);
        assert_eq!(s, 200);
        assert!(!served.as_array().expect("profiles").is_empty());
        for row in served.as_array().expect("profiles") {
            let disk = read(root, "dashboard", row["id"].as_str().expect("id"));
            assert_eq!(disk["_rev"], expected);
            assert_eq!(row["_rev"], disk["_rev"]);
        }
    }
}
pub(super) fn cleanup_outside_lease() {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("contract-session-cleanup").expect("root");
    let p = repo(tmp.path())
        .save(profile("p"), 1000.0)
        .expect("profile");
    let other = repo(tmp.path())
        .save(profile("other"), 1000.0)
        .expect("other");
    let live = repo(tmp.path()).user_data_path(&p.id).expect("session");
    fs::write(live.join("Cookies"), "live").expect("bytes");
    let (reached_tx, reached_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    thread::scope(|scope| {
        let root = tmp.path().to_owned();
        let id = p.id.clone();
        let worker = scope.spawn(move || {
            with_observer(
                move |e| {
                    if e.operation == "before-session-reclaim" {
                        reached_tx.send(()).expect("reached cleanup");
                        release_rx
                            .recv_timeout(Duration::from_secs(15))
                            .expect("release cleanup");
                    }
                    Ok(Action::Continue)
                },
                || repo(&root).delete_session(&id, "source-contract-host"),
            )
        });
        reached_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("cleanup reached");
        let lease =
            SessionLease::acquire(&SessionLease::path_for(tmp.path(), &p.id).expect("lease path"))
                .expect("delete released session lease before cleanup");
        drop(lease);
        repo(tmp.path())
            .edit(
                &other.id,
                |p| {
                    p.notes = "went through".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("unrelated write outside cleanup");
        release_tx.send(()).expect("release");
        worker
            .join()
            .expect("delete thread")
            .expect("delete finished");
    });
    assert_eq!(
        repo(tmp.path())
            .resolve(&other.id, 1002.0)
            .expect("other read")
            .expect("other")
            .notes,
        "went through"
    );
}
pub(super) fn token_conflict() {
    let tmp = common::TempDirectory::new("contract-token-conflict").expect("root");
    let tokens = numera_core::http::tokens::TokenStore::new(tmp.path().to_owned());
    let made = tokens.create("CI runner", 1000.0).expect("first token");
    assert!(
        tokens
            .create("  CI runner  ", 1001.0)
            .expect_err("duplicate token")
            .to_string()
            .contains("already exists")
    );
    let auth = format!("Bearer {}", made["token"].as_str().expect("token"));
    let (s, v) = http_request(
        tmp.path(),
        "POST",
        "/api/tokens",
        encode(&json!({"name":"CI runner"})),
        Some(&auth),
    );
    assert_eq!(s, 409, "{v}");
    assert!(
        v["detail"]
            .as_str()
            .expect("detail")
            .contains("already exists")
    );
}
