use super::*;
use axum::http::HeaderMap;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};

struct Site {
    address: SocketAddr,
    stop: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Site {
    fn new() -> io::Result<Self> {
        Self::at("127.0.0.1:0".parse().map_err(io::Error::other)?)
    }
    fn at(bind: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = stop.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match listener.accept() {
                    Ok((stream, _)) => respond(stream, &signal)?,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error),
                }
                let (lock, changed) = &*signal;
                let done = lock
                    .lock()
                    .map_err(|_| io::Error::other("Fixture lock failed"))?;
                if *done {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::other("HTTP fixture deadline exceeded"));
                }
                let (done, _) = changed
                    .wait_timeout(done, Duration::from_millis(5))
                    .map_err(|_| io::Error::other("Fixture wait failed"))?;
                if *done {
                    return Ok(());
                }
            }
        });
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }
    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://{}{path}", self.address)).expect("Fixture URL")
    }
    fn finish(&mut self) -> io::Result<()> {
        let (lock, changed) = &*self.stop;
        *lock
            .lock()
            .map_err(|_| io::Error::other("Fixture lock failed"))? = true;
        changed.notify_all();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("HTTP fixture panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Site {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("HTTP fixture cleanup failed: {error}");
        }
    }
}
fn respond(mut stream: TcpStream, stop: &Arc<(Mutex<bool>, Condvar)>) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    let deadline = Instant::now() + Duration::from_secs(2);
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Fixture request deadline exceeded",
            ));
        }
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..count]);
        if request.len() > 8192 {
            return Err(io::Error::other("Fixture request exceeds limit"));
        }
    }
    let request = std::str::from_utf8(&request).map_err(io::Error::other)?;
    let path = request
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::other("Fixture path missing"))?;
    let (status, headers, body) = match path {
        "idle.invalid:443" => {
            assert!(request.starts_with("CONNECT idle.invalid:443 HTTP/1.1"));
            stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
            let (lock, changed) = &**stop;
            let done = lock
                .lock()
                .map_err(|_| io::Error::other("Fixture lock failed"))?;
            let _ = changed
                .wait_timeout_while(done, Duration::from_secs(5), |done| !*done)
                .map_err(|_| io::Error::other("Fixture wait failed"))?;
            return Ok(());
        }
        path if path.starts_with("http://destination.invalid/") => {
            assert!(request.contains("numera-proxy-check/1.0"));
            assert!(request.split("\r\n").any(|line| {
                line.split_once(':').is_some_and(|(name, value)| {
                    name.eq_ignore_ascii_case("proxy-authorization")
                        && value.trim() == "Basic dXNlcjpzeW50aGV0aWM="
                })
            }));
            (
                "200 OK",
                "",
                br#"{"ip":"192.0.2.7","country_code":"DE","city":"Fixture"}"#.as_slice(),
            )
        }
        "/rotate-first" => {
            assert!(request.contains("numera-proxy-rotate/1.0"));
            assert!(request.to_ascii_lowercase().contains("host: first.test:"));
            let port = stream.local_addr()?.port();
            write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: http://second.test:{port}/rotate-next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )?;
            return Ok(());
        }
        "/rotate-next" => {
            assert_eq!(
                stream.local_addr()?.ip().to_string(),
                "127.0.0.2",
                "Redirect reused first-hop pin"
            );
            assert!(request.to_ascii_lowercase().contains("host: second.test:"));
            assert!(request.contains("numera-proxy-rotate/1.0"));
            ("200 OK", "", b"rotation queued".as_slice())
        }
        "/rotate-missing" => ("302 Found", "", b"must not succeed".as_slice()),
        "/rotate-uri" => ("302 Found", "URI: /rotate-next\r\n", b"".as_slice()),
        "/redirect" => ("302 Found", "Location: /crx\r\n", b"".as_slice()),
        "/loop" => ("302 Found", "Location: /loop\r\n", b"".as_slice()),
        "/crx" => {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("user-agent: mozilla/5.0")
            );
            ("200 OK", "", b"Cr24synthetic".as_slice())
        }
        "/html" => ("200 OK", "", b"not-an-extension".as_slice()),
        "/large" => ("200 OK", "", b"0123456789".as_slice()),
        "/chunked" => (
            "200 OK",
            "Transfer-Encoding: chunked\r\n",
            b"a\r\n0123456789\r\n0\r\n\r\n".as_slice(),
        ),
        "/slow" => {
            stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nx",
            )?;
            let (lock, changed) = &**stop;
            let done = lock
                .lock()
                .map_err(|_| io::Error::other("Fixture lock failed"))?;
            let _ = changed
                .wait_timeout_while(done, Duration::from_secs(2), |done| !*done)
                .map_err(|_| io::Error::other("Fixture wait failed"))?;
            return Ok(());
        }
        _ => ("404 Not Found", "", b"missing".as_slice()),
    };
    let length = if path == "/chunked" {
        String::new()
    } else {
        format!("Content-Length: {}\r\n", body.len())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{headers}{length}Connection: close\r\n\r\n"
    )?;
    stream.write_all(body)
}

#[test]
fn rotation_control_revalidates_redirect_pin_host_and_missing_header() -> io::Result<()> {
    let mut site = Site::new()?;
    let mut next_site = Site::at(SocketAddr::new(
        "127.0.0.2".parse().map_err(io::Error::other)?,
        site.address.port(),
    ))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let port = site.address.port();
        let first = Url::parse(&format!("http://first.test:{port}/rotate-first"))
            .map_err(io::Error::other)?;
        let mut hops = Vec::new();
        let reply =
            proxy_rotate::control_with(first, site.address.ip(), Duration::from_secs(2), |url| {
                hops.push(url.host_str().unwrap_or("").to_owned());
                let address = next_site.address.ip();
                std::future::ready(Ok((url, address)))
            })
            .await?;
        assert_eq!(reply, "rotation queued");
        assert_eq!(hops, ["second.test"]);
        let second = |path| {
            Url::parse(&format!("http://second.test:{port}{path}")).map_err(io::Error::other)
        };
        let uri_reply = proxy_rotate::control_with(
            second("/rotate-uri")?,
            next_site.address.ip(),
            Duration::from_secs(2),
            |url| std::future::ready(Ok((url, next_site.address.ip()))),
        )
        .await?;
        assert_eq!(uri_reply, "rotation queued");
        let missing = proxy_rotate::control_with(
            second("/rotate-missing")?,
            site.address.ip(),
            Duration::from_secs(2),
            |_| -> std::future::Ready<io::Result<(Url, std::net::IpAddr)>> {
                panic!("Missing redirect header invoked resolver")
            },
        )
        .await
        .expect_err("302 without destination became success");
        assert!(missing.to_string().contains("no destination"));
        let refused = proxy_rotate::control_with(
            second("/rotate-uri")?,
            site.address.ip(),
            Duration::from_secs(2),
            |_| {
                std::future::ready(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Fixture private destination",
                )))
            },
        )
        .await
        .expect_err("Refused redirect reached transport");
        assert!(refused.to_string().contains("refused to follow a redirect"));
        Ok::<_, io::Error>(())
    });
    drop(runtime);
    let first_cleanup = site.finish();
    let next_cleanup = next_site.finish();
    first_cleanup?;
    next_cleanup?;
    result
}

#[test]
fn native_http_relay_owns_auth_connections_and_closes_idle_clients() -> io::Result<()> {
    let mut site = Site::new()?;
    let proxy = crate::proxy::Proxy::from_record(
        &serde_json::json!({
            "server":format!("http://{}",site.address),"username":"user","password":"synthetic","country":null
        }),
        false,
    )
    .map_err(io::Error::other)?;
    let mut relay = relay::Relay::start(&proxy)?;
    let address = Url::parse(&relay.url()).map_err(io::Error::other)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let proxy = serde_json::json!({"type":"HTTP","host":"127.0.0.1","port":address.port().ok_or_else(|| io::Error::other("Relay port missing"))?.to_string()});
        let result = proxy_check::check_at(&proxy, Duration::from_secs(2), &["http://destination.invalid/echo"]).await;
        assert_eq!(result["ok"], true);
        assert_eq!(result["ip"], "192.0.2.7");
        Ok::<_, io::Error>(())
    });
    drop(runtime);
    let mut idle = TcpStream::connect((
        "127.0.0.1",
        address
            .port()
            .ok_or_else(|| io::Error::other("Relay port missing"))?,
    ))?;
    idle.set_read_timeout(Some(Duration::from_secs(2)))?;
    idle.set_write_timeout(Some(Duration::from_secs(2)))?;
    idle.write_all(b"CONNECT idle.invalid:443 HTTP/1.1\r\nHost: idle.invalid:443\r\n\r\n")?;
    let expected = b"HTTP/1.1 200 Connection established\r\n\r\n";
    let mut ready = vec![0; expected.len()];
    idle.read_exact(&mut ready)?;
    assert_eq!(ready, expected, "Idle tunnel never admitted");
    relay.close(Duration::from_secs(2))?;
    relay.close(Duration::from_secs(2))?;
    assert_eq!(
        idle.read(&mut [0; 1])?,
        0,
        "Admitted idle tunnel survived shutdown"
    );
    assert!(
        TcpStream::connect_timeout(&idle.peer_addr()?, Duration::from_millis(100)).is_err(),
        "Relay listener survived shutdown"
    );
    drop(idle);
    site.finish()?;
    result
}

#[test]
fn stalled_dns_cancels_and_the_owned_runtime_shuts_down() -> io::Result<()> {
    use hickory_resolver::{
        TokioResolver,
        config::{NameServerConfig, ResolverConfig},
        net::runtime::TokioRuntimeProvider,
    };
    use reqwest::dns::{Name, Resolve};
    let blackhole = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let address = blackhole.local_addr()?;
    blackhole.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut server = NameServerConfig::udp(address.ip());
    server.connections[0].port = address.port();
    let config = ResolverConfig::from_parts(None, Vec::new(), vec![server]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let started = Instant::now();
    let result = runtime.block_on(async {
        let resolver = TokioResolver::builder_with_config(config, TokioRuntimeProvider::default())
            .build()
            .map_err(io::Error::other)?;
        let resolver = dns::SystemDns {
            resolver,
            budget: Duration::from_millis(200),
        };
        let result = resolver
            .resolve(
                "timeout-fixture.test."
                    .parse::<Name>()
                    .map_err(io::Error::other)?,
            )
            .await;
        let error = result
            .err()
            .ok_or_else(|| io::Error::other("Blackhole DNS unexpectedly resolved"))?;
        assert!(error.to_string().contains("deadline exceeded"));
        Ok::<_, io::Error>(())
    });
    drop(runtime);
    let mut datagram = [0u8; 512];
    let (count, sender) = blackhole.recv_from(&mut datagram)?;
    assert!(count >= 12, "Fixture did not observe an actual DNS query");
    assert!(
        sender.ip().is_loopback(),
        "DNS fixture query escaped loopback"
    );
    drop(blackhole);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "DNS runtime did not settle within its bound"
    );
    result
}

#[test]
fn native_download_streams_redirects_and_refuses_size_magic_or_stalls() -> io::Result<()> {
    let mut site = Site::new()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let result = runtime.block_on(async {
        let package = extension_download::download_from(id, site.url("/redirect"), Duration::from_secs(2), 32).await?;
        assert_eq!(package, b"Cr24synthetic");
        let error = extension_download::download_from(id, site.url("/html"), Duration::from_secs(2), 32).await.expect_err("HTML accepted as CRX");
        assert_eq!(error.to_string(), format!("the store did not return an extension for id '{id}' - check the id is correct and the extension is still published."));
        for path in ["/large", "/chunked"] {
            let error = get(&client(Duration::from_secs(2))?, site.url(path), HeaderMap::new(), 4).await.expect_err("Oversized response accepted");
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
        let proxy = serde_json::json!({"type":"HTTP","host":"127.0.0.1","port":site.address.port().to_string(),"user":"user","pass":"synthetic"});
        let result = proxy_check::check_at(&proxy, Duration::from_secs(2), &["http://destination.invalid/echo"]).await;
        assert_eq!(result["ok"], true);
        assert_eq!(result["ip"], "192.0.2.7");
        assert_eq!(result["country"], "DE");
        assert_eq!(result["city"], "Fixture");
        assert!(result["latencyMs"].as_u64().is_some());
        assert!(client(Duration::ZERO).is_err());
        let error = get(&client(Duration::from_secs(2))?, site.url("/large"), HeaderMap::new(), 0).await.expect_err("Zero output limit accepted");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        for path in ["/loop", "/missing?token=synthetic-sensitive"] {
            let error = get(&client(Duration::from_secs(2))?, site.url(path), HeaderMap::new(), 32).await.expect_err("Failed HTTP request accepted");
            assert!(!error.to_string().contains("synthetic-sensitive"));
            assert!(!error.to_string().contains(&site.address.to_string()));
        }
        assert!(extension_download::download_sync("invalid").is_err(), "Invalid ID reached download from entered runtime");
        let start = Instant::now();
        let error = get(&client(Duration::from_millis(200))?, site.url("/slow"), HeaderMap::new(), 32).await.expect_err("Stalled response accepted");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
        Ok(())
    });
    site.finish()?;
    result
}
