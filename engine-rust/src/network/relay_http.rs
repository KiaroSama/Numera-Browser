use super::relay::Upstream;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

async fn head(client: &mut TcpStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let rest = bytes.split_off(end + 4);
            bytes.truncate(end);
            return Ok((bytes, rest));
        }
        if bytes.len() >= 65536 {
            return Err(io::Error::other("request head too large"));
        }
        let count = client
            .read(&mut buffer[..8192.min(65536 - bytes.len())])
            .await?;
        if count == 0 {
            return Err(io::Error::other("client closed before sending a request"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}
fn target(target: &str, connect: bool) -> io::Result<(String, u16, String)> {
    if connect {
        let (host, port) = target
            .rsplit_once(':')
            .ok_or_else(|| io::Error::other("CONNECT target is not host:port"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(io::Error::other("CONNECT target is not host:port"));
        }
        let port = port
            .parse::<u16>()
            .map_err(|_| io::Error::other("CONNECT port is invalid"))?;
        return Ok((host.into(), port, String::new()));
    }
    let url = reqwest::Url::parse(target)
        .map_err(|_| io::Error::other("expected an absolute URL from the browser"))?;
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| io::Error::other("browser request has no host"))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let mut path = if url.path().is_empty() {
        "/".into()
    } else {
        url.path().to_owned()
    };
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    Ok((
        host.trim_start_matches('[').trim_end_matches(']').into(),
        port,
        path,
    ))
}
fn rebuilt(head: &[u8], line: &[u8], upstream: &Upstream, connect: bool) -> Vec<u8> {
    let mut bytes = line.to_vec();
    for header in head.split(|byte| *byte == b'\n').skip(1) {
        let header = header.strip_suffix(b"\r").unwrap_or(header);
        let lower = header.to_ascii_lowercase();
        if header.is_empty()
            || [
                b"connection:".as_slice(),
                b"proxy-connection:",
                b"proxy-authorization:",
                b"keep-alive:",
            ]
            .iter()
            .any(|key| lower.starts_with(key))
        {
            continue;
        }
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(header);
    }
    if !upstream.socks()
        && let Some(user) = &upstream.user
    {
        bytes.extend_from_slice(b"\r\nProxy-Authorization: Basic ");
        bytes.extend_from_slice(
            STANDARD
                .encode(format!(
                    "{user}:{}",
                    upstream.password.as_deref().unwrap_or("")
                ))
                .as_bytes(),
        );
    }
    if !connect {
        bytes.extend_from_slice(b"\r\nConnection: close");
    }
    bytes.extend_from_slice(b"\r\n\r\n");
    bytes
}
async fn reply(client: &mut TcpStream, status: u16, reason: &str, message: &str) {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}",
        message.len()
    );
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.write_all(response.as_bytes()),
    )
    .await;
}

pub(super) async fn serve(
    mut client: TcpStream,
    upstream: std::sync::Arc<Upstream>,
) -> io::Result<()> {
    let request = tokio::time::timeout(std::time::Duration::from_secs(15), head(&mut client)).await;
    let (head, rest) = match request {
        Ok(Ok(request)) => request,
        _ => return Ok(()),
    };
    let parsed = (|| {
        let line = head.split(|byte| *byte == b'\n').next().unwrap_or(&head);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let text: String = line.iter().map(|byte| char::from(*byte)).collect();
        let mut parts = text.split_whitespace();
        let method = parts
            .next()
            .ok_or_else(|| io::Error::other("request method missing"))?;
        let target_text = parts
            .next()
            .ok_or_else(|| io::Error::other("request target missing"))?;
        let version = parts
            .next()
            .ok_or_else(|| io::Error::other("request version missing"))?;
        let connect = method.eq_ignore_ascii_case("CONNECT");
        let (host, port, path) = target(target_text, connect)?;
        let line = if upstream.socks() && !connect {
            format!("{method} {path} {version}").into_bytes()
        } else {
            line.to_vec()
        };
        Ok::<_, io::Error>((host, port, connect, line))
    })();
    let (host, port, connect, line) = match parsed {
        Ok(parsed) => parsed,
        Err(_) => {
            reply(
                &mut client,
                400,
                "Bad Request",
                "the relay could not read that request",
            )
            .await;
            return Ok(());
        }
    };
    let tunnel = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        upstream.open(&host, port),
    )
    .await;
    let tunnel = match tunnel {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Relay upstream deadline exceeded",
        )),
    };
    let mut tunnel = match tunnel {
        Ok(tunnel) => tunnel,
        Err(error) => {
            upstream.failed(&error);
            reply(
                &mut client,
                502,
                "Bad Gateway",
                "relay could not reach the upstream proxy",
            )
            .await;
            return Ok(());
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        if upstream.socks() && connect {
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await?;
        } else {
            tunnel
                .write_all(&rebuilt(&head, &line, &upstream, connect))
                .await?;
        }
        tunnel.write_all(&rest).await?;
        Ok::<_, io::Error>(())
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "Relay request write deadline exceeded",
        )
    })??;
    tokio::io::copy_bidirectional(&mut client, &mut tunnel).await?;
    Ok(())
}
