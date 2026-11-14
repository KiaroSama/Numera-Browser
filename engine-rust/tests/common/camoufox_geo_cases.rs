use numera_core::browser::sdk::camoufox_geo::{self, firefox_preferences, geolocation_at};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct FixtureDirectory(PathBuf);
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn put(directory: &Path, name: &str) -> io::Result<()> {
    fs::write(
        directory.join("mmdb").join(name),
        include_bytes!("../fixtures/camoufox-geo.mmdb"),
    )
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn options(value: Value) -> Result<Map<String, Value>, Box<dyn Error>> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| "Options are not an object".into())
}
type ProxyWorker = thread::JoinHandle<io::Result<Vec<Vec<u8>>>>;
fn rejecting_proxy(socks4: bool) -> io::Result<(SocketAddr, ProxyWorker)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let worker = thread::spawn(move || -> io::Result<Vec<Vec<u8>>> {
        let started = Instant::now();
        let mut requests = Vec::new();
        while requests.len() < 6 && started.elapsed() < Duration::from_secs(10) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    let mut request = Vec::new();
                    if socks4 {
                        let mut header = [0; 8];
                        stream.read_exact(&mut header)?;
                        request.extend_from_slice(&header);
                        for _ in 0..2 {
                            loop {
                                let mut byte = [0];
                                stream.read_exact(&mut byte)?;
                                request.push(byte[0]);
                                if byte[0] == 0 {
                                    break;
                                }
                                if request.len() > 1024 {
                                    return Err(io::Error::other(
                                        "SOCKS4 fixture request exceeds limit",
                                    ));
                                }
                            }
                        }
                        stream.write_all(&[0, 91, 0, 0, 0, 0, 0, 0])?;
                    } else {
                        while !request.ends_with(b"\r\n\r\n") && request.len() < 4096 {
                            let mut byte = [0];
                            stream.read_exact(&mut byte)?;
                            request.push(byte[0]);
                        }
                        stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
                    }
                    requests.push(request);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error),
            }
        }
        Ok(requests)
    });
    Ok((address, worker))
}
/// Invoked from the retained browser-policy case; does not add a test identity.
pub fn verify() -> Result<(), Box<dyn Error>> {
    let data: Value = serde_json::from_str(include_str!("../fixtures/camoufox-geo.json"))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalogs = root.join("src/browser/sdk/camoufox-data");
    let digests = data["catalog_sha256"]
        .as_object()
        .ok_or("Missing locale catalog digests")?;
    assert_eq!(digests.len(), 2);
    for (name, expected) in digests {
        assert!(!name.contains(['/', '\\']));
        assert_eq!(
            digest(&fs::read(catalogs.join(name))?),
            expected.as_str().ok_or("Invalid digest")?
        );
    }
    assert_eq!(
        digest(include_bytes!("../fixtures/camoufox-geo.mmdb")),
        data["mmdb_sha256"].as_str().ok_or("Missing MMDB digest")?
    );
    let cases = data["locales"].as_array().ok_or("Missing locale cases")?;
    assert_eq!(cases.len(), 3804);
    for case in cases {
        let actual = camoufox_geo::locale_config_at(
            &case["locale"],
            case["roll"].as_f64().ok_or("Missing locale roll")?,
        );
        if case["error"] == true {
            assert!(
                actual.is_err(),
                "Source rejects locale {} roll={}",
                case["locale"],
                case["roll"]
            );
        } else {
            assert_eq!(
                json!(actual?),
                case["config"],
                "Locale {} roll={}",
                case["locale"],
                case["roll"]
            );
        }
    }
    let preference_cases = data["preferences"]
        .as_array()
        .ok_or("Missing source preference cases")?;
    assert_eq!(preference_cases.len(), 6);
    for case in preference_cases {
        assert_eq!(
            json!(firefox_preferences(&options(case["config"].clone())?)),
            case["preferences"]
        );
    }
    for roll in [-1.0, 1.0, f64::NAN, f64::INFINITY] {
        assert!(camoufox_geo::locale_config_at(&json!("en-US"), roll).is_err());
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temp = FixtureDirectory(
        std::env::temp_dir().join(format!("numera-geo-{}-{unique}", std::process::id())),
    );
    fs::create_dir_all(temp.0.join("mmdb"))?;
    put(&temp.0, "geoip aio by daijro-combined.mmdb")?;
    let expected = data["geo_fields"].clone();
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "203.0.113.1", 0.5)?),
        expected
    );
    assert_eq!(
        json!(geolocation_at(
            &temp.0,
            Some("GEOIP AIO BY DAIJRO"),
            "203.0.113.1",
            0.5
        )?),
        expected
    );
    let mut ipv6 = expected.clone();
    ipv6["timezone"] = "Pacific/Auckland".into();
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "2001:db8::1", 0.5)?),
        ipv6
    );
    for absent in ["203.0.113.2", "2001:db8::3"] {
        assert_eq!(
            geolocation_at(&temp.0, None, absent, 0.5)
                .expect_err("Unknown address must fail")
                .kind(),
            io::ErrorKind::AddrNotAvailable
        );
    }
    assert_eq!(
        geolocation_at(&temp.0, None, "2001:db8::2", 0.5)
            .expect_err("Incomplete record must fail")
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(geolocation_at(&temp.0, None, "not-an-ip", 0.5).is_err());
    assert!(geolocation_at(&temp.0, Some("not-a-source"), "203.0.113.1", 0.5).is_err());
    assert!(geolocation_at(&temp.0.join("absent"), None, "203.0.113.1", 0.5).is_err());
    // Old implicit GeoLite2 choices migrate to the current default. Explicit
    // choices and per-call selections retain their separate IPv4/IPv6 paths.
    fs::write(temp.0.join("config.yml"), "name: MaxMind GeoLite2\n")?;
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "203.0.113.1", 0.5)?),
        expected
    );
    fs::write(
        temp.0.join("config.yml"),
        "name: MaxMind GeoLite2\nexplicit: true\n",
    )?;
    assert!(geolocation_at(&temp.0, None, "203.0.113.1", 0.5).is_err());
    put(&temp.0, "maxmind geolite2-ipv4.mmdb")?;
    put(&temp.0, "maxmind geolite2-ipv6.mmdb")?;
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "203.0.113.1", 0.5)?),
        expected
    );
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "2001:db8::1", 0.5)?),
        ipv6
    );
    let saved = fs::read(temp.0.join("config.yml"))?;
    assert_eq!(
        json!(geolocation_at(
            &temp.0,
            Some("GeoIP AIO by daijro"),
            "203.0.113.1",
            0.5
        )?),
        expected
    );
    assert_eq!(fs::read(temp.0.join("config.yml"))?, saved);
    fs::write(
        temp.0.join("config.yml"),
        "name: Local Fixture\nurls:\n  combined: unused\npaths:\n  iso_code: country.iso_code\n  longitude: location.longitude\n  latitude: location.latitude\n  timezone: location.time_zone\n",
    )?;
    put(&temp.0, "local fixture-combined.mmdb")?;
    assert_eq!(
        json!(geolocation_at(&temp.0, None, "203.0.113.1", 0.5)?),
        expected
    );
    fs::write(
        temp.0.join("mmdb/local fixture-combined.mmdb"),
        b"not a database",
    )?;
    assert_eq!(
        geolocation_at(&temp.0, None, "203.0.113.1", 0.5)
            .expect_err("Corrupt database must fail")
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::remove_file(temp.0.join("config.yml"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let explicit = options(json!({"geoip":"2001:db8::1","proxy":{"server":"http://127.0.0.1:1"},"locale":"fr-FR","config":{"timezone":"Europe/Paris","locale:language":"de","locale:region":"DE","webrtc:ipv6":"2001:db8::9","geolocation:latitude":99}}))?;
        let actual = camoufox_geo::resolve_in(&explicit, Some(&temp.0)).await?;
        assert_eq!(actual["timezone"], "Europe/Paris");
        assert_eq!(actual["locale:language"], "fr");
        assert_eq!(actual["locale:region"], "FR");
        assert_eq!(actual["locale:script"], "Latn");
        assert_eq!(actual["geolocation:latitude"], 2.5);
        assert_eq!(actual["webrtc:ipv6"], "2001:db8::9");
        assert!(!actual.contains_key("network:ipv4Only"));
        let prefs = firefox_preferences(&actual);
        assert_eq!(prefs["permissions.default.geo"], 1);
        assert_eq!(prefs["intl.locale.requested"], "fr-Latn-FR");
        let blocked = camoufox_geo::resolve_in(&options(json!({"geoip":"2001:db8::1","block_webrtc":true}))?, Some(&temp.0)).await?;
        assert!(!blocked.contains_key("webrtc:ipv6"));
        let (proxy_address, deny_proxy) = rejecting_proxy(false)?;
        let failed_proxy = camoufox_geo::resolve_in(&options(json!({"geoip":true,"proxy":{"server":format!("http://{proxy_address}"),"username":"private-user","password":"private-password"}}))?, Some(&temp.0)).await.expect_err("Failed proxy must not use the host connection");
        assert_eq!(deny_proxy.join().map_err(|_| "Proxy fixture thread failed")??.len(), 6);
        assert_eq!(failed_proxy.kind(), io::ErrorKind::AddrNotAvailable);
        assert!(!failed_proxy.to_string().contains("private"));
        let (socks_address, deny_socks) = rejecting_proxy(true)?;
        let failed_socks = camoufox_geo::resolve_in(&options(json!({"geoip":true,"proxy":{"server":format!("socks4://source%2Duser@{socks_address}")}}))?, Some(&temp.0)).await.expect_err("Failed SOCKS4 proxy must not use the host connection");
        assert_eq!(failed_socks.kind(), io::ErrorKind::AddrNotAvailable);
        let requests = deny_socks.join().map_err(|_| "SOCKS4 fixture thread failed")??;
        assert_eq!(requests.len(), 6);
        for request in requests {
            assert_eq!(&request[..8], &[4,1,1,187,0,0,0,1]);
            let user_end = request[8..].iter().position(|byte| *byte == 0).ok_or("SOCKS4 user delimiter missing")? + 8;
            assert_eq!(&request[8..user_end], b"source-user");
        }
        let no_geo = camoufox_geo::resolve_in(&options(json!({"geoip":false,"locale":"es-419"}))?, None).await?;
        assert_eq!(no_geo["locale:region"], "419");
        assert!(!no_geo.contains_key("timezone"));
        assert_eq!(firefox_preferences(&Map::new()), options(json!({"intl.locale.requested":"en-US"}))?);
        let missing = camoufox_geo::resolve_in(&options(json!({"geoip":true}))?, None).await.expect_err("Missing resources must fail before network requests");
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        Ok::<(),Box<dyn Error>>(())
    })?;
    Ok(())
}
