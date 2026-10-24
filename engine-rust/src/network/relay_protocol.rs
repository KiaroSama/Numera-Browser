use std::io;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn failure(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
async fn exact<const N: usize>(stream: &mut TcpStream) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| failure("SOCKS5 upstream closed the connection mid-handshake"))?;
    Ok(bytes)
}
pub(super) async fn socks4(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    user: Option<&str>,
) -> io::Result<()> {
    let user = user.unwrap_or("").as_bytes();
    if user.contains(&0) {
        return Err(failure("SOCKS4 user id may not contain a NUL byte"));
    }
    if user.len() > 255 {
        return Err(failure("SOCKS4 user id may not exceed 255 bytes"));
    }
    let original_host = host;
    let shown = crate::validation::repr(&serde_json::Value::String(host.into()));
    let host = if host.is_ascii() {
        host.to_owned()
    } else {
        reqwest::Url::parse(&format!("http://{host}/"))
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .ok_or_else(|| failure("hostname does not fit a SOCKS4a request"))?
    };
    let host = host.as_bytes();
    if host.is_empty() || host.len() > 255 || host.contains(&0) {
        return Err(failure(format!(
            "hostname does not fit a SOCKS4a request: {shown}"
        )));
    }
    let mut request = vec![4, 1];
    request.extend_from_slice(&port.to_be_bytes());
    request.extend_from_slice(&[0, 0, 0, 1]);
    request.extend_from_slice(user);
    request.push(0);
    request.extend_from_slice(host);
    request.push(0);
    stream.write_all(&request).await?;
    let reply = exact::<8>(stream).await?;
    if reply[0] != 0 {
        return Err(failure(format!(
            "upstream did not answer as SOCKS4 (version byte {})",
            reply[0]
        )));
    }
    if reply[1] != 90 {
        let reason: String = match reply[1] {
            91 => "request rejected or failed".into(),
            92 => "rejected: the proxy could not reach an identd on this machine".into(),
            93 => "rejected: the proxy's identd disagreed with the user id we sent".into(),
            code => code.to_string(),
        };
        return Err(failure(format!(
            "SOCKS4 upstream refused CONNECT to {original_host}:{port} ({reason})"
        )));
    }
    Ok(())
}
pub(super) async fn socks5(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    user: Option<&str>,
    password: Option<&str>,
) -> io::Result<()> {
    stream
        .write_all(if user.is_some_and(|user| !user.is_empty()) {
            &[5, 2, 0, 2]
        } else {
            &[5, 1, 0]
        })
        .await?;
    let [version, method] = exact::<2>(stream).await?;
    if version != 5 {
        return Err(failure(format!(
            "upstream did not answer as SOCKS5 (version byte {version})"
        )));
    }
    match method {
        0 => {}
        2 => {
            let user = user.unwrap_or("").as_bytes();
            let password = password.unwrap_or("").as_bytes();
            if user.len() > 255 || password.len() > 255 {
                return Err(failure("SOCKS5 username/password may not exceed 255 bytes"));
            }
            let mut request = vec![1, user.len() as u8];
            request.extend_from_slice(user);
            request.push(password.len() as u8);
            request.extend_from_slice(password);
            stream.write_all(&request).await?;
            if exact::<2>(stream).await?[1] != 0 {
                return Err(failure("SOCKS5 upstream rejected the credentials"));
            }
        }
        255 => {
            return Err(failure(format!(
                "SOCKS5 upstream accepted none of our authentication methods{}",
                if user.is_some_and(|user| !user.is_empty()) {
                    ""
                } else {
                    " — it likely needs a username and password"
                }
            )));
        }
        method => {
            return Err(failure(format!(
                "SOCKS5 upstream asked for authentication method {method}, which the relay does not speak"
            )));
        }
    }
    let shown = crate::validation::repr(&serde_json::Value::String(host.into()));
    let host = host.as_bytes();
    if host.is_empty() || host.len() > 255 {
        return Err(failure(format!(
            "hostname does not fit a SOCKS5 request: {shown}"
        )));
    }
    let mut request = vec![5, 1, 0, 3, host.len() as u8];
    request.extend_from_slice(host);
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;
    let [_, reply, _, address] = exact::<4>(stream).await?;
    if reply != 0 {
        let reason: String = match reply {
            1 => "general SOCKS server failure".into(),
            2 => "connection not allowed by ruleset".into(),
            3 => "network unreachable".into(),
            4 => "host unreachable".into(),
            5 => "connection refused".into(),
            6 => "TTL expired".into(),
            7 => "command not supported".into(),
            8 => "address type not supported".into(),
            code => code.to_string(),
        };
        return Err(failure(format!(
            "SOCKS5 upstream refused CONNECT to {}:{port} ({reason})",
            String::from_utf8_lossy(host)
        )));
    }
    let length = match address {
        1 => 4,
        4 => 16,
        3 => usize::from(exact::<1>(stream).await?[0]),
        address => {
            return Err(failure(format!(
                "SOCKS5 upstream sent unknown address type {address}"
            )));
        }
    };
    let mut remaining = vec![0; length + 2];
    stream.read_exact(&mut remaining).await.map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            failure("SOCKS5 upstream closed the connection mid-handshake")
        } else {
            error
        }
    })?;
    Ok(())
}
