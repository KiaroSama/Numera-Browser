use super::{files, Service};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct Bridge {
    endpoint: String,
    token: String,
    origin: String,
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Bridge {
    pub fn start(service: Arc<Service>, engine_port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let origin = format!("http://127.0.0.1:{engine_port}");
        let token = files::nonce()?;
        let stop = Arc::new(AtomicBool::new(false));
        let ending = stop.clone();
        let expected_origin = origin.clone();
        let expected_token = token.clone();
        let ready = Arc::new(AtomicBool::new(false));
        let readiness = ready.clone();
        let thread = thread::spawn(move || {
            let mut last_check: Option<std::time::Instant> = None;
            while !ending.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, address)) if address.ip().is_loopback() => {
                        let _ = answer(&mut stream, &expected_origin, &expected_token, &service);
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                if readiness.load(Ordering::Acquire)
                    && last_check
                        .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(86400))
                {
                    let _ = service.check();
                    last_check = Some(std::time::Instant::now());
                }
                thread::park_timeout(Duration::from_millis(25));
            }
            service.shutdown();
        });
        Ok(Self {
            endpoint,
            token,
            origin,
            stop,
            ready,
            thread: Some(thread),
        })
    }
    pub fn ready(&self) {
        self.ready.store(true, Ordering::Release);
    }
    #[cfg(feature = "update-acceptance")]
    pub fn report_acceptance(&self, root: &std::path::Path, engine_port: u16) -> io::Result<()> {
        super::acceptance::report_bridge(root, engine_port, &self.endpoint, &self.token)
    }
    pub fn injection(&self) -> String {
        let config = json!({"endpoint":self.endpoint,"token":self.token});
        format!("if(window===window.top&&location.origin==={}){{Object.defineProperty(window,'__NUMERA_UPDATES__',{{value:{},configurable:true}});window.dispatchEvent(new Event('numera-updates-ready'));}}",serde_json::to_string(&self.origin).unwrap_or_default(),config)
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}
fn equal(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
fn answer(
    stream: &mut TcpStream,
    origin: &str,
    token: &str,
    service: &Arc<Service>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
        if bytes.len() >= 8192 {
            return Err(io::Error::other("Updater bridge header too large"));
        }
        let mut block = [0; 1024];
        let count = stream.read(&mut block)?;
        if count == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&block[..count]);
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).map_err(io::Error::other)?;
    let mut lines = headers.split("\r\n");
    let request = lines
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>();
    if request.len() != 3 || request[2] != "HTTP/1.1" {
        return Err(io::Error::other("Malformed updater bridge request"));
    }
    let method = request[0].to_owned();
    let route = request[1].to_owned();
    let mut seen = std::collections::BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| io::Error::other("Malformed updater bridge header"))?;
        if seen
            .insert(key.to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            return Err(io::Error::other("Duplicate updater bridge header"));
        }
    }
    if seen.get("origin").map(String::as_str) != Some(origin) {
        return response(
            stream,
            403,
            None,
            &json!({"error":"Untrusted update origin"}),
        );
    }
    if method == "OPTIONS" {
        return response(stream, 204, Some(origin), &json!({}));
    }
    if !seen
        .get("authorization")
        .is_some_and(|value| equal(value, &format!("Bearer {token}")))
    {
        return response(
            stream,
            403,
            Some(origin),
            &json!({"error":"Untrusted update caller"}),
        );
    }
    let size = seen
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(io::Error::other)?
        .unwrap_or(0);
    if size > 4096 || seen.contains_key("transfer-encoding") {
        return Err(io::Error::other("Updater request body exceeds its limit"));
    }
    while bytes.len() < header_end + size {
        let mut block = [0; 1024];
        let count = stream.read(&mut block)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Update request ended early",
            ));
        }
        bytes.extend_from_slice(&block[..count]);
    }
    let payload: Value = if size == 0 {
        json!({})
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + size]).map_err(io::Error::other)?
    };
    let id = payload["offerId"].as_str().unwrap_or("");
    let approve = payload["approve"] == true;
    let result = match (method.as_str(), route.as_str()) {
        ("GET", "/status") => Ok(()),
        ("POST", "/check") => service.check(),
        ("POST", "/download") => service.download(id, approve),
        ("POST", "/install") => service.install(id, approve),
        ("POST", "/cancel") => service.cancel(id),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No such update operation",
        )),
    };
    let mut status = serde_json::to_value(service.status()).map_err(io::Error::other)?;
    let code = match result {
        Ok(()) => 200,
        Err(error) => {
            status["message"] = if error.kind() == io::ErrorKind::WouldBlock {
                "Another update operation is active."
            } else {
                "Update request refused; verify the current offer and explicit approval."
            }
            .into();
            if error.kind() == io::ErrorKind::WouldBlock {
                409
            } else {
                400
            }
        }
    };
    response(stream, code, Some(origin), &status)
}
fn response(
    stream: &mut TcpStream,
    status: u16,
    origin: Option<&str>,
    value: &Value,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    let cors=origin.map(|origin|format!("Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: authorization, content-type\r\nVary: Origin\r\n")).unwrap_or_default();
    write!(stream,"HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{cors}\r\n",if status<400{"OK"}else{"Refused"},bytes.len())?;
    stream.write_all(&bytes)?;
    stream.flush()
}
