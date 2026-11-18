//! Development-only HTTP forwarding. Production is served by the engine.
use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
};
pub const API_ORIGIN: &str = "127.0.0.1:8787";
fn read_request(client: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut request = vec![];
    let mut buffer = [0; 16384];
    let mut expected = None;
    loop {
        let n = client.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..n]);
        if request.len() > 16 * 1024 * 1024 {
            return Err(io::Error::other("request exceeds 16 MiB"));
        }
        if expected.is_none()
            && let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n")
        {
            let header = std::str::from_utf8(&request[..end]).map_err(io::Error::other)?;
            let content_length = header
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
                .map(|(_, v)| v.trim().parse::<usize>())
                .transpose()
                .map_err(io::Error::other)?
                .unwrap_or(0);
            expected = Some(end + 4 + content_length);
        }
        if expected.is_some_and(|expected| request.len() >= expected) {
            break;
        }
    }
    Ok(request)
}
pub fn serve_connection(mut client: TcpStream, root: &Path, api_origin: &str) -> io::Result<()> {
    let request = read_request(&mut client)?;
    let end = request
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| io::Error::other("incomplete HTTP headers"))?;
    let header = std::str::from_utf8(&request[..end]).map_err(io::Error::other)?;
    let path = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    if path.starts_with("/api/") {
        let mut backend = TcpStream::connect(api_origin)?;
        for line in header.lines() {
            if !line
                .split_once(':')
                .is_some_and(|(k, _)| k.eq_ignore_ascii_case("Connection"))
            {
                write!(backend, "{line}\r\n")?;
            }
        }
        backend.write_all(b"Connection: close\r\n\r\n")?;
        backend.write_all(&request[end + 4..])?;
        io::copy(&mut backend, &mut client)?;
        return Ok(());
    }
    let relative = path
        .split('?')
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let requested = root.join(if relative.is_empty() {
        "index.html"
    } else {
        relative
    });
    let file = fs::canonicalize(&requested)
        .ok()
        .filter(|p| p.starts_with(root) && p.is_file())
        .unwrap_or_else(|| root.join("index.html"));
    let body = fs::read(&file)?;
    let mime = match file.extension().and_then(|e| e.to_str()) {
        Some("wasm") => "application/wasm",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "text/html",
    };
    write!(
        client,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    client.write_all(&body)
}
pub fn run(root: &Path) -> io::Result<()> {
    let root = fs::canonicalize(root)?;
    let listener = TcpListener::bind("127.0.0.1:5173")?;
    println!("Dashboard http://127.0.0.1:5173; /api → http://{API_ORIGIN}");
    for client in listener.incoming() {
        let root = root.clone();
        if let Ok(client) = client {
            thread::spawn(move || {
                if let Err(err) = serve_connection(client, &root, API_ORIGIN) {
                    eprintln!("Dashboard development request failed: {err}");
                }
            });
        }
    }
    Ok(())
}
