use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
#[derive(Clone, Debug)]
pub(super) struct Request {
    pub line: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
pub(super) fn request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut bytes = Vec::new();
    let mut b = [0; 8192];
    let end = loop {
        if let Some(p) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
            break p + 4;
        }
        if bytes.len() > 65536 {
            return Err(io::Error::other("fixture head limit"));
        }
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request closed",
            ));
        }
        bytes.extend_from_slice(&b[..n]);
    };
    let head = String::from_utf8(bytes[..end].to_vec()).map_err(io::Error::other)?;
    let mut lines = head.split("\r\n");
    let line = lines.next().unwrap_or("").to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let length = headers
        .get("content-length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = bytes[end..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "body closed"));
        }
        body.extend_from_slice(&b[..n]);
    }
    Ok(Request {
        line,
        headers,
        body,
    })
}
pub(super) fn reply(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )?;
    for (k, v) in headers {
        write!(stream, "{k}: {v}\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    stream.write_all(body)?;
    stream.flush()
}
pub(super) struct Server {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
}
impl Server {
    pub fn start(handler: impl Fn(TcpStream) -> io::Result<()> + Send + Sync + 'static) -> Self {
        Self::at("127.0.0.1:0".parse().expect("loopback"), handler).expect("local fixture")
    }
    pub fn at(
        bind: SocketAddr,
        handler: impl Fn(TcpStream) -> io::Result<()> + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let sockets = Arc::new(Mutex::new(Vec::<TcpStream>::new()));
        let active = sockets.clone();
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if stopped.load(Ordering::Acquire) {
                            break;
                        }
                        // accept() may inherit the listener's nonblocking mode
                        // on macOS/Windows; these protocol handlers use blocking IO.
                        stream.set_nonblocking(false)?;
                        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                        stream.set_nodelay(true)?;
                        let mut registered = active
                            .lock()
                            .map_err(|_| io::Error::other("fixture socket lock"))?;
                        // Serialize admission with finish()'s shutdown pass.
                        // An accept completing after stop must never escape it.
                        if stopped.load(Ordering::Acquire) {
                            break;
                        }
                        registered.push(stream.try_clone()?);
                        drop(registered);
                        let call = handler.clone();
                        workers.push(thread::spawn(move || call(stream)));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => return Err(e),
                }
            }
            for worker in workers {
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e))
                        if [
                            io::ErrorKind::BrokenPipe,
                            io::ErrorKind::ConnectionReset,
                            io::ErrorKind::UnexpectedEof,
                            io::ErrorKind::WouldBlock,
                            io::ErrorKind::TimedOut,
                        ]
                        .contains(&e.kind()) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(_) => return Err(io::Error::other("fixture worker panicked")),
                }
            }
            Ok(())
        });
        Ok(Self {
            address,
            stop,
            sockets,
            worker: Some(worker),
        })
    }
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
    pub fn finish(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        for stream in self
            .sockets
            .lock()
            .map_err(|_| io::Error::other("fixture socket lock"))?
            .iter()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("fixture listener panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Err(error) = self.finish()
            && !thread::panicking()
        {
            panic!("native network fixture: {error}");
        }
    }
}
pub(super) struct Site {
    pub server: Server,
    pub seen: Arc<Mutex<Vec<Request>>>,
}
impl Site {
    pub fn new() -> Self {
        Self::body(None)
    }
    pub fn body(fixed: Option<Vec<u8>>) -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observed = seen.clone();
        let server = Server::start(move |mut stream| {
            let r = request(&mut stream)?;
            let target = r.line.split_whitespace().nth(1).unwrap_or("");
            let body = fixed.clone().unwrap_or_else(|| {
                format!("NUMERA_TEST_ORIGIN {target} body={}", r.body.len()).into_bytes()
            });
            observed
                .lock()
                .map_err(|_| io::Error::other("fixture request lock"))?
                .push(r);
            reply(&mut stream, 200, &[], &body)
        });
        Self { server, seen }
    }
    pub fn url(&self, path: &str) -> String {
        self.server.url(path)
    }
}
pub(super) fn exchange(
    url: &str,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, String) {
    let u = reqwest::Url::parse(url).expect("relay URL");
    let mut stream = TcpStream::connect((
        u.host_str().expect("host"),
        u.port_or_known_default().expect("port"),
    ))
    .expect("local connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(stream,"{method} {target} HTTP/1.1\r\nHost: numera.test\r\nContent-Length: {}\r\nConnection: close\r\n",body.len()).expect("request line");
    for (k, v) in headers {
        write!(stream, "{k}: {v}\r\n").expect("header");
    }
    stream.write_all(b"\r\n").expect("head end");
    stream.write_all(body).expect("request body");
    response(&mut stream)
}
pub(super) fn response(stream: &mut TcpStream) -> (u16, String) {
    let r = request(stream).expect("response");
    let status = r
        .line
        .split_whitespace()
        .nth(1)
        .expect("status")
        .parse()
        .expect("numeric status");
    (status, String::from_utf8_lossy(&r.body).into_owned())
}
pub(super) fn duplex(a: TcpStream, b: TcpStream) -> io::Result<()> {
    let mut a_read = a.try_clone()?;
    let mut b_write = b.try_clone()?;
    thread::scope(|scope| {
        let first = scope.spawn(move || {
            let r = io::copy(&mut a_read, &mut b_write);
            let _ = a_read.shutdown(Shutdown::Both);
            let _ = b_write.shutdown(Shutdown::Both);
            r
        });
        let mut b_read = b;
        let mut a_write = a;
        let other = io::copy(&mut b_read, &mut a_write);
        let _ = b_read.shutdown(Shutdown::Both);
        let _ = a_write.shutdown(Shutdown::Both);
        let first = first
            .join()
            .map_err(|_| io::Error::other("fixture pipe panicked"))?;
        for r in [first, other] {
            if let Err(e) = r
                && ![
                    io::ErrorKind::BrokenPipe,
                    io::ErrorKind::ConnectionReset,
                    io::ErrorKind::WouldBlock,
                    io::ErrorKind::TimedOut,
                ]
                .contains(&e.kind())
            {
                return Err(e);
            }
        }
        Ok(())
    })
}
pub(super) struct HttpProxy {
    pub server: Server,
    pub seen: Arc<Mutex<Vec<(String, String)>>>,
}
impl HttpProxy {
    pub fn new() -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observed = seen.clone();
        let server = Server::start(move |mut client| {
            let r = request(&mut client)?;
            let auth = r
                .headers
                .get("proxy-authorization")
                .and_then(|s| s.strip_prefix("Basic "))
                .and_then(|s| STANDARD.decode(s).ok())
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            observed
                .lock()
                .map_err(|_| io::Error::other("seen lock"))?
                .push((r.line.clone(), auth.clone()));
            if auth != "relay-user:s3cr3t pass" {
                return reply(
                    &mut client,
                    407,
                    &[("Proxy-Authenticate", "Basic realm=fixture".into())],
                    b"",
                );
            }
            if r.line.starts_with("CONNECT ") {
                let target = r.line.split_whitespace().nth(1).unwrap_or("");
                let upstream = TcpStream::connect(target)?;
                client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
                duplex(client, upstream)
            } else {
                let target = r.line.split_whitespace().nth(1).unwrap_or("");
                reply(
                    &mut client,
                    200,
                    &[],
                    format!("NUMERA_TEST_ORIGIN {target}").as_bytes(),
                )
            }
        });
        Self { server, seen }
    }
    pub fn model(&self, password: &str) -> Proxy {
        Proxy::from_record(
            &json!({"server":self.server.url(""),"username":"relay-user","password":password}),
            false,
        )
        .expect("HTTP proxy")
    }
}
#[derive(Clone, Copy)]
pub(super) enum SocksKind {
    Four,
    Five,
}
pub(super) struct Socks {
    pub server: Server,
    pub seen: Arc<Mutex<Vec<Value>>>,
}
fn byte(stream: &mut TcpStream) -> io::Result<u8> {
    let mut b = [0];
    stream.read_exact(&mut b)?;
    Ok(b[0])
}
fn bytes(stream: &mut TcpStream, len: usize) -> io::Result<Vec<u8>> {
    let mut out = vec![0; len];
    stream.read_exact(&mut out)?;
    Ok(out)
}
fn cstring(stream: &mut TcpStream) -> io::Result<String> {
    let mut out = Vec::new();
    loop {
        let b = byte(stream)?;
        if b == 0 {
            break;
        }
        out.push(b);
        if out.len() > 1024 {
            return Err(io::Error::other("fixture C string limit"));
        }
    }
    String::from_utf8(out).map_err(io::Error::other)
}
impl Socks {
    pub fn new(kind: SocksKind, target: SocketAddr, auth: bool, accept: bool, grant: bool) -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observed = seen.clone();
        let server = Server::start(move |mut client| {
            let (host, port, user, address) = match kind {
                SocksKind::Five => {
                    let version = byte(&mut client)?;
                    let count = byte(&mut client)?;
                    let offered = bytes(&mut client, count as usize)?;
                    if version != 5 {
                        return Err(io::Error::other("not SOCKS5"));
                    }
                    client.write_all(if auth { &[5, 2] } else { &[5, 0] })?;
                    let mut user = String::new();
                    if auth {
                        let version = byte(&mut client)?;
                        let n = byte(&mut client)?;
                        user = String::from_utf8(bytes(&mut client, n as usize)?)
                            .map_err(io::Error::other)?;
                        let n = byte(&mut client)?;
                        let password = String::from_utf8(bytes(&mut client, n as usize)?)
                            .map_err(io::Error::other)?;
                        let ok = version == 1 && accept && user == "u" && password == "p";
                        observed.lock().map_err(|_|io::Error::other("seen lock"))?.push(json!({"auth":format!("{user}:{password}"),"authenticated":ok,"methods":offered}));
                        client.write_all(&[1, if ok { 0 } else { 1 }])?;
                        if !ok {
                            return Ok(());
                        }
                    }
                    let h = bytes(&mut client, 4)?;
                    let host = match h[3] {
                        1 => {
                            let a = bytes(&mut client, 4)?;
                            format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3])
                        }
                        3 => {
                            let n = byte(&mut client)?;
                            String::from_utf8(bytes(&mut client, n as usize)?)
                                .map_err(io::Error::other)?
                        }
                        _ => return Err(io::Error::other("unsupported fixture address")),
                    };
                    let p = bytes(&mut client, 2)?;
                    (host, u16::from_be_bytes([p[0], p[1]]), user, Value::Null)
                }
                SocksKind::Four => {
                    let h = bytes(&mut client, 8)?;
                    let user = cstring(&mut client)?;
                    let address = json!([h[4], h[5], h[6], h[7]]);
                    let host = if h[4..7] == [0, 0, 0] && h[7] != 0 {
                        cstring(&mut client)?
                    } else {
                        format!("{}.{}.{}.{}", h[4], h[5], h[6], h[7])
                    };
                    observed
                        .lock()
                        .map_err(|_| io::Error::other("seen lock"))?
                        .push(json!({"version":h[0],"command":h[1]}));
                    (host, u16::from_be_bytes([h[2], h[3]]), user, address)
                }
            };
            observed
                .lock()
                .map_err(|_| io::Error::other("seen lock"))?
                .push(
                    json!({"host":host,"port":port,"ident":user,"address":address,"connect":true}),
                );
            match kind {
                SocksKind::Four => {
                    client.write_all(&[0, if grant { 90 } else { 91 }, 0, 0, 0, 0, 0, 0])?
                }
                SocksKind::Five => {
                    client.write_all(&[5, if grant { 0 } else { 5 }, 0, 1, 0, 0, 0, 0, 0, 0])?
                }
            }
            if !grant {
                return Ok(());
            }
            let upstream = TcpStream::connect(target)?;
            duplex(client, upstream)
        });
        Self { server, seen }
    }
    pub fn model(&self, kind: SocksKind, user: Option<&str>, password: Option<&str>) -> Proxy {
        Proxy::from_record(&json!({"server":format!("{}://{}",match kind{SocksKind::Four=>"socks4",SocksKind::Five=>"socks5"},self.server.address),"username":user,"password":password}),false).expect("SOCKS proxy")
    }
}
pub(super) fn tunnel(relay: &Relay, origin: &Site, path: &str) -> (u16, String) {
    let url = reqwest::Url::parse(&relay.url().expect("ready relay URL")).expect("relay URL");
    let mut stream =
        TcpStream::connect(("127.0.0.1", url.port().expect("port"))).expect("relay socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    write!(
        stream,
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
        origin.server.address, origin.server.address
    )
    .expect("CONNECT");
    let established = request(&mut stream).expect("tunnel response");
    assert!(established.line.contains("200"), "{}", established.line);
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: origin\r\nConnection: close\r\n\r\n"
    )
    .expect("tunnel GET");
    response(&mut stream)
}
