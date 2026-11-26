//! Forward MCP stdio messages to a running service without replaying tool calls.
use crate::mcp;
use reqwest::{Client, Request, StatusCode, Url, header};
use serde_json::Value;
use std::{io, net::SocketAddr, time::Duration};

pub(crate) struct Bridge {
    client: Client,
    endpoint: Url,
    authorization: Option<header::HeaderValue>,
}

impl Bridge {
    pub(crate) fn new(url: &str, token: Option<&str>, allow_remote: bool) -> io::Result<Self> {
        let mut endpoint = Url::parse(url).map_err(|_| invalid("Invalid MCP bridge URL"))?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !matches!(endpoint.path(), "/" | "/api/mcp")
        {
            return Err(invalid(
                "Use an HTTP(S) origin or its /api/mcp endpoint without credentials, a query, or a fragment",
            ));
        }
        let token = token.filter(|value| !value.trim().is_empty());
        let localhost = endpoint.host_str() == Some("localhost");
        let loopback = localhost
            || endpoint.host_str().is_some_and(|host| {
                host.trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
        if !loopback && !(allow_remote && token.is_some() && endpoint.scheme() == "https") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Non-loopback MCP bridges require HTTPS, --allow-remote-bridge, and --token",
            ));
        }
        let authorization = token
            .map(|value| {
                if value.bytes().any(|byte| !byte.is_ascii_graphic()) {
                    return Err(invalid("Invalid MCP bridge bearer token"));
                }
                let mut header = header::HeaderValue::from_str(&format!("Bearer {value}"))
                    .map_err(|_| invalid("Invalid MCP bridge bearer token"))?;
                header.set_sensitive(true);
                Ok(header)
            })
            .transpose()?;
        let mut client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(mcp::CALL_TIMEOUT_SECONDS + 10));
        if localhost {
            client = client.resolve_to_addrs(
                "localhost",
                &[
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 0)),
                ],
            );
        }
        endpoint.set_path("/api/mcp");
        let client = client.build().map_err(transport)?;
        Ok(Self {
            client,
            endpoint,
            authorization,
        })
    }

    fn build_request(&self, message: &Value) -> io::Result<Request> {
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(mcp::encode(message)?);
        if let Some(authorization) = &self.authorization {
            request = request.header(header::AUTHORIZATION, authorization.clone());
        }
        request.build().map_err(transport)
    }

    pub(crate) async fn request(&self, message: &Value) -> io::Result<Option<Value>> {
        let mut response = self
            .client
            .execute(self.build_request(message)?)
            .await
            .map_err(transport)?;
        let status = response.status();
        if matches!(status, StatusCode::NO_CONTENT | StatusCode::ACCEPTED) {
            return Ok(None);
        }
        if status != StatusCode::OK {
            return Err(io::Error::other(format!(
                "MCP bridge returned HTTP {status}"
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if chunk.len() >= mcp::MAX_FRAME.saturating_sub(body.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP bridge response exceeds limit",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid MCP bridge response"))
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn transport(error: reqwest::Error) -> io::Error {
    let kind = if error.is_timeout() {
        io::ErrorKind::TimedOut
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, "MCP bridge transport failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use serde_json::json;

    #[test]
    fn remote_destination_requires_explicit_opt_in_token_and_tls() -> io::Result<()> {
        for (url, token, remote) in [
            ("https://example.test:8787", None, false),
            ("https://example.test:8787", Some("fixture"), false),
            ("https://example.test:8787", None, true),
            ("https://example.test:8787", Some("  "), true),
            ("http://example.test:8787", Some("fixture"), true),
            ("http://localhost.example.test:8787", None, false),
            ("http://user:pass@127.0.0.1:8787", None, false),
            ("http://127.0.0.1:8787/?token=secret", None, false),
        ] {
            assert!(Bridge::new(url, token, remote).is_err(), "accepted {url}");
        }
        for url in [
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
            "http://localhost:8787",
        ] {
            assert_eq!(Bridge::new(url, None, false)?.endpoint.path(), "/api/mcp");
        }
        assert!(Bridge::new("https://example.test:8787", Some("fixture"), true).is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn request_forwards_bearer_header_and_preserves_json_rpc() -> io::Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let bridge = Bridge::new(
            &format!("http://{}", listener.local_addr()?),
            Some("fixture"),
            false,
        )?;
        let app = Router::new().route("/api/mcp", post(|headers: HeaderMap, Json(message): Json<Value>| async move {
            Json(json!({"authorization":headers.get("authorization").and_then(|value| value.to_str().ok()),"message":message}))
        }));
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        let message = json!({"jsonrpc":"2.0","id":"qa-1","method":"ping"});
        let result = tokio::time::timeout(Duration::from_secs(3), bridge.request(&message)).await;
        let _ = stop.send(());
        let mut server = server;
        match tokio::time::timeout(Duration::from_secs(2), &mut server).await {
            Ok(joined) => joined.map_err(io::Error::other)??,
            Err(_) => {
                server.abort();
                let _ = server.await;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Bridge fixture did not stop",
                ));
            }
        }
        assert_eq!(
            result.map_err(io::Error::other)??,
            Some(json!({"authorization":"Bearer fixture","message":message}))
        );
        Ok(())
    }
}
