use numera_core::{network::relay::Relay, ownership::capture, proxy::Proxy};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
mod common;

const RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n\x00\xffwire";

struct Upstream {
    address: SocketAddr,
    stop: mpsc::Sender<()>,
    observed: mpsc::Receiver<Vec<u8>>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Upstream {
    fn start(count: usize) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (stop, stopped) = mpsc::channel();
        let (seen, observed) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            for _ in 0..count {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(error) => return Err(error),
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Relay fixture accept deadline",
                        ));
                    }
                    match stopped.recv_timeout(Duration::from_millis(2)) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => return Ok(()),
                    }
                };
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                let mut greeting = [0; 3];
                stream.read_exact(&mut greeting)?;
                if greeting != [5, 1, 0] {
                    return Err(io::Error::other(
                        "Relay fixture expected unauthenticated SOCKS5",
                    ));
                }
                stream.write_all(&[5, 0])?;
                let mut connect = [0; 5];
                stream.read_exact(&mut connect)?;
                if connect[..4] != [5, 1, 0, 3] {
                    return Err(io::Error::other("Relay destination was not remote DNS"));
                }
                let mut destination = vec![0; usize::from(connect[4]) + 2];
                stream.read_exact(&mut destination)?;
                if destination != b"destination.invalid\x1f\x90" {
                    return Err(io::Error::other("Relay destination identity changed"));
                }
                stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
                let mut head = Vec::new();
                let mut byte = [0];
                while !head.ends_with(b"\r\n\r\n") {
                    if head.len() >= 8192 || Instant::now() >= deadline {
                        return Err(io::Error::other("Relay fixture head bound"));
                    }
                    stream.read_exact(&mut byte)?;
                    head.push(byte[0]);
                }
                seen.send(head).map_err(io::Error::other)?;
                stream.write_all(RESPONSE)?;
            }
            Ok(())
        });
        Ok(Self {
            address,
            stop,
            observed,
            worker: Some(worker),
        })
    }
    fn finish(&mut self) -> io::Result<()> {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("Relay fixture worker panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Upstream {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("Relay fixture cleanup failed: {error}");
        }
    }
}

fn bytes(value: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    value
        .as_array()
        .ok_or("Original wire missing")?
        .iter()
        .map(|byte| {
            byte.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "Original wire byte invalid".into())
        })
        .collect()
}

#[test]
fn socks_origin_requests_preserve_original_target_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("relay-request-original")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.relay import _origin_request_line,_rebuild_head
from run_logging import run_log
paths=[b'/first/../second',b'/first/./second',b'/%2e%2E/item',b'/a/.%2e/item',
 b'/plain',b'/a//b///c',b'/%2f/%2F/%41',b'/a;b?x=1&x=2&encoded=%2f',
 b'/caf\xe9?q=\xff',b'/plain#fragment',b'',b'?x=1',b'/?',b'/path?',
 b'/path?x=1#fragment',b'/path#fragment?not-query',b'/a\\b?x=\\y',
 b'/a?question=?&empty=&plus=+',b'/path%zz?unchanged=%zz']
rows=[]
with run_log('relay-request-oracle',parent/'logs'):
 for path in paths:
  line=b'GET http://destination.invalid:8080'+path+b' HTTP/1.1'
  head=line+b'\r\nHost: destination.invalid:8080\r\nProxy-Authorization: Basic must-not-forward\r\nProxy-Connection: keep-alive\r\nConnection: keep-alive\r\nX-Fixture: retained\r\nKeep-Alive: timeout=5'
  rows.append({'request':list(head+b'\r\n\r\n'),
   'expected':list(_rebuild_head(head,_origin_request_line(line),None,True))})
print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path());
    let observed = match capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(20),
            idle: Duration::from_secs(15),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    ) {
        Ok(observed) => observed,
        Err(mut failure) => {
            if failure.settle(Duration::from_secs(5)).is_err() {
                std::mem::forget(temporary);
            }
            return Err(failure.into());
        }
    };
    assert_eq!(
        observed.exit,
        0,
        "Original relay observation failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 19);
    let mut upstream = Upstream::start(cases.len())?;
    let proxy = Proxy::from_record(
        &json!({"server":format!("socks5://{}",upstream.address)}),
        false,
    )?;
    let mut relay = Relay::start(&proxy)?;
    let address = reqwest::Url::parse(&relay.url())?;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        for (index, case) in cases.iter().enumerate() {
            let mut client =
                TcpStream::connect(("127.0.0.1", address.port().ok_or("Relay port missing")?))?;
            client.set_read_timeout(Some(Duration::from_secs(3)))?;
            client.set_write_timeout(Some(Duration::from_secs(2)))?;
            client.write_all(&bytes(&case["request"])?)?;
            let mut response = Vec::new();
            client.take(8192).read_to_end(&mut response)?;
            assert_eq!(response, RESPONSE, "Relay response bytes differ: {index}");
            let head = upstream.observed.recv_timeout(Duration::from_secs(2))?;
            assert_eq!(
                head,
                bytes(&case["expected"])?,
                "SOCKS origin request bytes differ: {index}"
            );
        }
        Ok(())
    })();
    let closed = relay.close(Duration::from_secs(3));
    let settled = upstream.finish();
    outcome?;
    closed?;
    settled?;
    temporary.cleanup()?;
    Ok(())
}
