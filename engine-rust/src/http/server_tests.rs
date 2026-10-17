use super::*;
use crate::common;
use std::io::{Read, Write};

fn options(root: &std::path::Path) -> Options {
    Options {
        root: root.into(),
        address: ([127, 0, 0, 1], 0).into(),
        token: String::new(),
        origins: String::new(),
        insecure_override: String::new(),
        password: None,
        host: "fixture-host".into(),
        installed: Default::default(),
        browser: Runtime {
            python: root.join("never-start-python"),
            adapter: root.join("never-start-sdk"),
            cwd: root.into(),
            home: root.into(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            deployment: None,
        },
        dashboard: None,
    }
}
#[test]
fn real_http_readiness_and_stop_close_the_listener_without_starting_an_sdk()
-> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("native-http-lifetime")?;
    let mut server = Server::start(options(root.path()))?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), Box<dyn std::error::Error>> {
            server.wait_ready(Duration::from_secs(5))?;
            let mut stream =
                std::net::TcpStream::connect_timeout(&server.address(), Duration::from_secs(2))?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            stream.write_all(
                b"GET /api/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )?;
            let mut bytes = Vec::new();
            stream.take(65537).read_to_end(&mut bytes)?;
            assert!(bytes.len() <= 65536);
            let text = String::from_utf8(bytes)?;
            assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
            let (_, body) = text
                .split_once("\r\n\r\n")
                .ok_or("HTTP response body missing")?;
            let body: serde_json::Value = serde_json::from_str(body)?;
            assert_eq!(
                body,
                serde_json::json!({"ok":true,"service":"numera-browser","version":env!("CARGO_PKG_VERSION"),"authRequired":false})
            );
            let logs =
                std::fs::read_dir(root.path().join("logs"))?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(logs.len(), 1, "Readiness launched an SDK log");
            assert!(
                logs[0]
                    .file_name()
                    .to_string_lossy()
                    .starts_with("native-http_")
            );
            Ok(())
        },
    ));
    let address = server.address();
    server.stop(Duration::from_secs(5))?;
    assert!(server.worker.is_none());
    assert!(server.settled);
    assert!(server.wait_ready(Duration::ZERO).is_err());
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    match result {
        Ok(result) => result?,
        Err(panic) => std::panic::resume_unwind(panic),
    }
    drop(server);
    root.cleanup()?;
    Ok(())
}
#[test]
fn failed_readiness_never_becomes_success_on_a_second_attempt()
-> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("http-failed-ready")?;
    let (send, ready) = std::sync::mpsc::sync_channel(1);
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    send.send(Err(io::Error::other("Controlled startup failure")))?;
    let sessions = Arc::new(Registry::new(
        root.path().into(),
        "fixture-host".into(),
        None,
    )?);
    let log = Arc::new(crate::runtime_log::RuntimeLog::new(
        &root.path().join("logs"),
        "native-http",
    ));
    let worker = thread::spawn(move || {
        wait.recv_timeout(Duration::from_secs(5))
            .map_err(io::Error::other)?;
        Ok(())
    });
    let mut server = Server {
        address: ([127, 0, 0, 1], 0).into(),
        worker: Some(worker),
        ready: Some(ready),
        stop: Arc::new(AtomicBool::new(false)),
        shutdown: None,
        sessions,
        cleaning: false,
        failure: None,
        log,
        settled: false,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(server.wait_ready(Duration::ZERO).is_err());
        drop(send);
        assert!(server.wait_ready(Duration::ZERO).is_err());
        let error = server
            .stop(Duration::ZERO)
            .expect_err("Blocked HTTP worker was forgotten");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(server.worker.is_some());
        assert!(!server.settled);
    }));
    let _ = release.send(());
    server.stop(Duration::from_secs(5))?;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    drop(server);
    root.cleanup()?;
    Ok(())
}
#[test]
fn bind_refusal_and_occupied_port_do_not_create_a_worker() -> Result<(), Box<dyn std::error::Error>>
{
    let root = common::TempDirectory::new("native-http-refusal")?;
    let mut refused = options(root.path());
    refused.address = ([0, 0, 0, 0], 0).into();
    let error = Server::start(refused)
        .err()
        .ok_or("Open remote bind permitted")?;
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let mut occupied = options(root.path());
    occupied.address = listener.local_addr()?;
    let error = Server::start(occupied).err().ok_or("Occupied port bound")?;
    assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    drop(listener);
    root.cleanup()?;
    Ok(())
}
