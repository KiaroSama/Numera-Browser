use super::*;
pub(super) fn locale_name(locales: &[&str], expected: &str) {
    let tmp = common::TempDirectory::new("source-net-locale").expect("root");
    let ext = tmp.path().join("ext");
    write_ext(
        &ext,
        json!({"name":"__MSG_extName__","default_locale":"fr"}),
        &[],
    );
    for locale in locales {
        let path = ext.join("_locales").join(locale);
        fs::create_dir_all(&path).expect("catalog directory");
        let name = match *locale {
            "en" => "English",
            "en_US" => "Regional",
            "fr" => "Default",
            _ => panic!("locale fixture"),
        };
        fs::write(
            path.join("messages.json"),
            serde_json::to_vec(&json!({"extName":{"message":name}})).expect("catalog JSON"),
        )
        .expect("catalog bytes");
    }
    assert_eq!(
        extension_catalog::describe(&ext).expect("description").name,
        expected
    );
}
pub(super) fn unsafe_locale(locale: Value) {
    use numera_core::vault::testing::{Action, with_observer};
    let tmp = common::TempDirectory::new("source-net-unsafe-locale").expect("root");
    let ext = tmp.path().join("ext");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside fixture");
    let payload = br#"{"extName":{"message":"Outside catalog must not be returned"}}"#;
    fs::write(outside.join("messages.json"), payload).expect("outside bytes");
    let selected = match locale.as_str() {
        Some("traversal") => json!("../../outside"),
        Some("absolute") => json!(outside),
        _ => locale,
    };
    write_ext(
        &ext,
        json!({"name":"__MSG_extName__","default_locale":selected}),
        &[("_locales/messages.json", payload)],
    );
    let description = with_observer(
        |e| {
            if e.operation == "before-extension-json-read"
                && e.path.file_name().is_some_and(|f| f == "messages.json")
            {
                return Err(io::Error::other("invalid locale reached catalog reading"));
            }
            Ok(Action::Continue)
        },
        || extension_catalog::describe(&ext),
    )
    .expect("invalid locale refused before reading");
    assert_eq!(description.name, "ext");
    assert_eq!(
        fs::read(outside.join("messages.json")).expect("protected catalog"),
        payload
    );
}
pub(super) fn link_directory(target: &Path, link: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("owned directory symlink");
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("owned junction creation");
        assert!(
            output.status.success(),
            "junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
pub(super) fn ext_flags(entries: Vec<String>, root: &Path) -> Vec<String> {
    let mut p = profile("extension flags");
    p.extensions = entries;
    args(&p, root)
}
pub(super) fn invalid_extension_path(kind: &str, door: &str) {
    let tmp = common::TempDirectory::new("source-net-extension-validation").expect("root");
    let target = tmp
        .path()
        .join(if kind == "missing" { "nope" } else { "empty" });
    if kind == "empty" {
        fs::create_dir(&target).expect("empty directory");
    }
    let outcome = if door == "config" {
        let repo = repository(tmp.path());
        let installed = BTreeMap::new();
        numera_core::cli::settings_commands::Settings {
            root: tmp.path(),
            repo: &repo,
            installed: &installed,
            environment_token: false,
        }
        .config(
            json!({"default_extension":[target]})
                .as_object()
                .expect("arguments"),
            tmp.path(),
        )
        .expect("CLI config result")
    } else {
        numera_core::cli::extension_commands::defaults(
            tmp.path(),
            "default-add",
            Some(target.to_str().expect("path")),
            tmp.path(),
        )
        .expect("CLI default result")
    };
    assert_eq!(outcome.exit, 1, "{}", outcome.payload);
    assert!(stored_defaults(tmp.path()).is_empty());
}
pub(super) fn add_extension(
    root: &Path,
    path: &str,
) -> numera_core::cli::profile_commands::Outcome {
    let p = repository(root)
        .save(profile("target"), 1000.0)
        .expect("profile");
    commands(root, |c| {
        c.extensions("add", &p.id, Some(path), root, 1001.0)
    })
    .expect("extension command")
}
pub(super) fn split_server(text: &str, want: (&str, &str, &str)) {
    let parsed = pool::parse_line(text).expect("native proxy endpoint parse");
    assert_eq!(
        parsed["type"].as_str().expect("type").to_lowercase(),
        want.0
    );
    assert_eq!(parsed["host"], want.1);
    assert_eq!(parsed["port"], want.2);
}
pub(super) fn legacy_rotate(url: &str, door: &str) {
    let tmp = common::TempDirectory::new("source-net-legacy-rotate").expect("root");
    let store = ProxyPool::new(tmp.path().to_owned(), None);
    let mut rows = store
        .add_many(vec![json!({"type":"HTTP","host":"1.2.3.4","port":"8080"})])
        .expect("proxy");
    let id = rows[0]["id"].as_str().expect("ID").to_owned();
    rows[0]["rotateUrl"] = json!(url);
    fs::write(
        tmp.path().join("proxies.json"),
        serde_json::to_vec_pretty(&rows).expect("legacy JSON"),
    )
    .expect("legacy bytes");
    if door == "http" {
        let (s, v) = json_http(
            tmp.path(),
            "POST",
            &format!("/api/proxies/{id}/rotate"),
            Value::Null,
        );
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["rotated"], false);
        assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()));
    } else {
        let result = commands(tmp.path(), |c| {
            let pool = numera_core::cli::pool_commands::Pool {
                store: &store,
                profiles: c,
            };
            pool.rotate(
                &[id],
                |p| rotation::rotate_sync(p, Duration::from_secs(1)),
                || 1000.0,
            )
        })
        .expect("native CLI rotate result");
        assert_ne!(result.exit, 0);
        assert!(!result.payload.to_string().contains("Traceback"));
    }
}
pub(super) fn socks_health(mode: &str, auth: bool) {
    let origin = Site::body(Some(
        br#"{"ip":"203.0.113.7","country_code":"US"}"#.to_vec(),
    ));
    let socks = Socks::new(SocksKind::Five, origin.server.address, auth, true, true);
    let p = json!({"type":"SOCKS5","host":"127.0.0.1","port":socks.server.address.port(),"user":if auth{Some("u")}else{None},"pass":if auth{Some("p")}else{None}});
    let endpoint = format!("http://echo.example:{}/json", origin.server.address.port());
    let res = runtime(proxy_check::testing::check_at(
        &p,
        Duration::from_secs(3),
        &[&endpoint],
    ));
    assert_eq!(res["ok"], true, "{res}");
    let seen = socks.seen.lock().expect("SOCKS events");
    assert!(
        seen.iter().any(|e| e["connect"] == true),
        "health check bypassed SOCKS"
    );
    if auth {
        assert!(seen.iter().any(|e| e["authenticated"] == true));
    }
    if mode == "verdict" {
        assert!(!res["ok"].is_null());
    }
}
pub(super) fn dns_row(country: &str) -> Value {
    json!({"ip":"1.1.1.1","country":country,"country_code":country,"asn":"AS1","type":"dns"})
}
pub(super) fn dns_grade(rows: Vec<Value>, country: Option<&str>) -> Value {
    numera_core::browser::probe_grades::dns(&json!({"rows":rows}), country)
        .expect("native DNS grading")
}
pub(super) fn open_tunnel(relay: &Relay, target: SocketAddr) -> TcpStream {
    let url = reqwest::Url::parse(&relay.url()).expect("relay URL");
    let mut socket =
        TcpStream::connect(("127.0.0.1", url.port().expect("port"))).expect("relay socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(
        socket,
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n"
    )
    .expect("CONNECT");
    let response = request(&mut socket).expect("CONNECT response");
    assert!(response.line.contains("200"), "{}", response.line);
    socket
}
pub(super) fn socks4_relay(user: Option<&str>, grant: bool) -> (Site, Socks, Relay) {
    let site = Site::new();
    let socks = Socks::new(SocksKind::Four, site.server.address, false, true, grant);
    let relay = Relay::start(&socks.model(SocksKind::Four, user, None)).expect("SOCKS4 relay");
    (site, socks, relay)
}
pub(super) fn failed_socks_check(auth: bool, accept: bool, grant: bool) -> Value {
    let origin = Site::body(Some(br#"{"ip":"203.0.113.7"}"#.to_vec()));
    let socks = Socks::new(SocksKind::Five, origin.server.address, auth, accept, grant);
    let entry = json!({"type":"SOCKS5","host":"127.0.0.1","port":socks.server.address.port(),"user":if auth{Some("u")}else{None},"pass":if auth{Some("wrong")}else{None}});
    let endpoint = origin.url("/ip");
    runtime(proxy_check::testing::check_at(
        &entry,
        Duration::from_secs(3),
        &[&endpoint],
    ))
}
pub(super) fn dead_socks_check(kind: &str) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("owned dead port");
    let address = listener.local_addr().expect("dead address");
    drop(listener);
    let entry = json!({"type":kind,"host":"127.0.0.1","port":address.port()});
    let result = runtime(proxy_check::testing::check_at(
        &entry,
        Duration::from_secs(2),
        &["http://echo.example/ip"],
    ));
    assert_eq!(result["ok"], false);
    let error = result["error"].as_str().expect("error");
    assert!(!error.is_empty());
    assert!(!error.contains("502"));
}
pub(super) fn rotation_entry(url: Option<&str>, previous: Option<&str>) -> Value {
    let mut p =
        json!({"id":"p1","type":"HTTP","host":"127.0.0.1","port":1,"user":null,"pass":null});
    if let Some(url) = url {
        p["rotateUrl"] = json!(url);
    }
    if let Some(ip) = previous {
        p["lastCheck"] = json!({"ip":ip});
    }
    p
}
pub(super) fn pool_added(root: &Path, hosts: &[&str], password: &str) -> Vec<Value> {
    let (s, v) = json_http(
        root,
        "POST",
        "/api/proxies",
        json!({"proxies":hosts.iter().map(|host|json!({"type":"HTTP","host":host,"port":8080,"user":"u","pass":password})).collect::<Vec<_>>()}),
    );
    assert_eq!(s, 200, "{v}");
    v["proxies"]
        .as_array()
        .expect("added proxies")
        .iter()
        .map(|p| p["id"].clone())
        .collect()
}
pub(super) fn pool_raw(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("proxies.json")).expect("raw protected pool"))
        .expect("pool JSON")
}
pub(super) fn pool_sealed(root: &Path, hosts: &[&str]) -> Vec<Value> {
    let ids = pool_added(root, hosts, "secret");
    repository(root)
        .enable_vault("master")
        .expect("real vault seal");
    assert_ne!(pool_raw(root)[0]["pass"], "secret");
    ids
}
pub(super) fn pool_sweep(root: &Path, body: Value) -> (u16, Value, Vec<String>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    let (s, v) = proxy_check::testing::with_checker(
        move |proxy| {
            observed
                .lock()
                .expect("checked hosts")
                .push(proxy["host"].as_str().expect("host").to_owned());
            json!({"ok":true,"ip":"203.0.113.7","ms":12})
        },
        || json_http(root, "POST", "/api/proxies/test", body),
    );
    let hosts = seen.lock().expect("checked hosts").clone();
    (s, v, hosts)
}
pub(super) fn redirect_pins(private: bool) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let handler = move |mut stream: TcpStream| {
        let actual = request(&mut stream)?;
        assert!(actual.line.starts_with("GET /next "));
        counted.fetch_add(1, Ordering::AcqRel);
        reply(&mut stream, 200, &[], b"second-pin")
    };
    let second = Server::at(
        "127.0.0.2:0".parse().expect("second loopback"),
        handler.clone(),
    )
    .or_else(|_| Server::at("[::1]:0".parse().expect("IPv6 loopback"), handler))
    .expect("distinct local redirect address");
    let target = format!("http://host-b.example:{}/next", second.address.port());
    let first = Server::start(move |mut stream| {
        let actual = request(&mut stream)?;
        assert!(actual.line.starts_with("GET /rotate "));
        reply(&mut stream, 302, &[("Location", target.clone())], b"")
    });
    assert_ne!(first.address.ip(), second.address.ip());
    let initial = format!("http://host-a.example:{}/rotate", first.address.port());
    let (url, pin) = rotation::testing::local_destination(&initial, &[first.address.ip()])
        .expect("initial validated local address");
    let redirect_address = second.address.ip();
    let result = runtime(rotation::testing::control_at(
        url,
        pin,
        Duration::from_secs(3),
        move |next| async move {
            if private {
                rotation::testing::public_destination(next.as_str(), &[redirect_address])
            } else {
                rotation::testing::local_destination(next.as_str(), &[redirect_address])
            }
        },
    ));
    if private {
        let error = result.expect_err("redirect to a private address must be refused");
        assert!(error.to_string().contains("refused to follow a redirect"));
        assert!(error.to_string().contains("not a public address"));
        assert_eq!(hits.load(Ordering::Acquire), 0);
    } else {
        assert_eq!(result.expect("redirect request"), "second-pin");
        assert_eq!(hits.load(Ordering::Acquire), 1);
    }
}
pub(super) fn environment_proxy_isolation() {
    let test_name = thread::current()
        .name()
        .expect("reported test name")
        .to_owned();
    const MARKER: &str = "NUMERA_SOURCE_NETWORK_ENV_CASE";
    if std::env::var(MARKER).as_deref() == Ok(test_name.as_str()) {
        let origin = Site::body(Some(b"direct".to_vec()));
        let initial = format!(
            "http://host-a.example:{}/rotate",
            origin.server.address.port()
        );
        let (url, address) =
            rotation::testing::local_destination(&initial, &[origin.server.address.ip()])
                .expect("actual origin pin");
        let body = runtime(rotation::testing::control_at(
            url,
            address,
            Duration::from_secs(3),
            |_| async { Err(io::Error::other("unexpected redirect")) },
        ))
        .expect("direct request despite ambient proxy settings");
        assert_eq!(body, "direct");
        assert_eq!(origin.seen.lock().expect("origin requests").len(), 1);
        return;
    }
    let dead = TcpListener::bind("127.0.0.1:0").expect("owned proxy port");
    let proxy_url = format!("http://{}", dead.local_addr().expect("proxy address"));
    drop(dead);
    let mut child =
        std::process::Command::new(std::env::current_exe().expect("native test binary"));
    child
        .args(["--exact", &test_name, "--nocapture", "--test-threads=1"])
        .env(MARKER, &test_name)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy");
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "http_proxy",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        child.env(key, &proxy_url);
    }
    let output = child
        .output()
        .expect("isolated native proxy-environment test");
    assert!(
        output.status.success(),
        "child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
