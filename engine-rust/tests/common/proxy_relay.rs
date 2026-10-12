use numera_core::{network::relay::Relay, proxy::Proxy};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub fn observe(case: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let expected = case["wire"]
        .as_array()
        .ok_or("Source relay wire missing")?
        .iter()
        .map(|byte| {
            byte.as_u64()
                .filter(|byte| *byte <= 255)
                .map(|byte| byte as u8)
                .ok_or("Source wire byte invalid")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let kind = case["kind"]
        .as_str()
        .ok_or("Relay kind missing")?
        .to_owned();
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let model = Proxy::from_record(
        &json!({"server":format!("{}://{address}",case["kind"].as_str().ok_or("Kind vanished")?),"username":"tenant-7",
        "password":if case["kind"] == "socks5" { json!("synthetic") } else { Value::Null }}),
        false,
    )?;
    let denied = case["denied"].as_bool().ok_or("Source denial missing")?;
    let (stop, stopped) = mpsc::channel::<()>();
    let worker = thread::spawn(move || -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("SOCKS fixture accept deadline"));
            }
            match stopped.recv_timeout(Duration::from_millis(5)) {
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                _ => return Err(io::Error::other("SOCKS fixture cancelled")),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        let mut wire = Vec::new();
        if kind == "socks5" {
            let mut hello = [0; 4];
            stream.read_exact(&mut hello)?;
            wire.extend_from_slice(&hello);
            stream.write_all(&[5, 2])?;
            let mut header = [0; 2];
            stream.read_exact(&mut header)?;
            wire.extend_from_slice(&header);
            let mut user = vec![0; usize::from(header[1])];
            stream.read_exact(&mut user)?;
            wire.extend_from_slice(&user);
            let mut length = [0];
            stream.read_exact(&mut length)?;
            wire.extend_from_slice(&length);
            let mut password = vec![0; usize::from(length[0])];
            stream.read_exact(&mut password)?;
            wire.extend_from_slice(&password);
            stream.write_all(&[1, 0])?;
            let mut header = [0; 5];
            stream.read_exact(&mut header)?;
            wire.extend_from_slice(&header);
            let mut target = vec![0; usize::from(header[4]) + 2];
            stream.read_exact(&mut target)?;
            wire.extend_from_slice(&target);
            stream.write_all(&[5, if denied { 5 } else { 0 }, 0, 1, 0, 0, 0, 0, 0, 0])?;
        } else {
            let mut header = [0; 8];
            stream.read_exact(&mut header)?;
            wire.extend_from_slice(&header);
            for _ in 0..2 {
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte)?;
                    wire.push(byte[0]);
                    if byte[0] == 0 {
                        break;
                    }
                    if wire.len() > 1024 {
                        return Err(io::Error::other("SOCKS fixture field limit"));
                    }
                }
            }
            stream.write_all(&[0, if denied { 91 } else { 90 }, 0, 0, 0, 0, 0, 0])?;
        }
        if denied {
            return Ok(wire);
        }
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            if request.len() >= 8192 || Instant::now() >= deadline {
                return Err(io::Error::other("SOCKS fixture request limit"));
            }
            let mut bytes = [0; 1024];
            let count = stream.read(&mut bytes)?;
            if count == 0 {
                return Err(io::Error::other("Relay request vanished"));
            }
            request.extend_from_slice(&bytes[..count]);
        }
        assert!(
            request.starts_with(b"GET /echo HTTP/1.1\r\n"),
            "SOCKS relay did not use origin form"
        );
        assert!(
            !String::from_utf8_lossy(&request)
                .to_ascii_lowercase()
                .contains("proxy-authorization")
        );
        let body = br#"{"ip":"192.0.2.8","country":"US"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
        Ok(wire)
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<_, Box<dyn std::error::Error>> {
            let mut relay = Relay::start(&model)?;
            let url = reqwest::Url::parse(&relay.url())?;
            // Public check uses production echo URLs, so this fixture owns its raw request.
            let mut client =
                TcpStream::connect(("127.0.0.1", url.port().ok_or("Relay port missing")?))?;
            client.set_read_timeout(Some(Duration::from_secs(2)))?;
            client.set_write_timeout(Some(Duration::from_secs(2)))?;
            client.write_all(b"GET http://destination.invalid:8080/echo HTTP/1.1\r\nHost: destination.invalid:8080\r\nConnection: close\r\n\r\n")?;
            let mut response = Vec::new();
            client.take(8192).read_to_end(&mut response)?;
            if denied {
                assert!(response.starts_with(b"HTTP/1.1 502 Bad Gateway\r\n"));
                assert_eq!(relay.last_error().as_deref(), case["error"].as_str());
                relay.close(Duration::from_secs(2))?;
                return Ok(());
            }
            let expected_body = br#"{"ip":"192.0.2.8","country":"US"}"#;
            let expected_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                expected_body.len()
            );
            assert_eq!(
                response,
                [expected_response.as_bytes(), expected_body].concat(),
                "Relay truncated or changed upstream response"
            );
            relay.close(Duration::from_secs(2))?;
            Ok(())
        },
    ));
    let _ = stop.send(());
    let joined = worker.join();
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    outcome?;
    let wire = joined.map_err(|_| "SOCKS fixture panicked")??;
    assert_eq!(wire, expected, "Original/native SOCKS wire diverged");
    Ok(())
}
