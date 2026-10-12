//! A loopback relay owns its runtime, connections and shutdown acknowledgement.
use super::{dns::SystemDns, relay_http, relay_protocol};
use crate::proxy::Proxy;
use reqwest::Url;
use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinSet,
};

pub(super) struct Upstream {
    kind: String,
    host: String,
    port: u16,
    pub(super) user: Option<String>,
    pub(super) password: Option<String>,
    dns: SystemDns,
    last_error: Arc<Mutex<Option<String>>>,
}
impl Upstream {
    pub(super) fn failed(&self, error: &io::Error) {
        if let Ok(mut last) = self.last_error.lock() {
            *last = Some(
                error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(180)
                    .collect(),
            );
        }
    }
    pub(super) fn socks(&self) -> bool {
        self.kind.starts_with("socks")
    }
    async fn connect(&self) -> io::Result<TcpStream> {
        let addresses = match self.host.parse::<std::net::IpAddr>() {
            Ok(address) => vec![address],
            Err(_) => self.dns.lookup(&self.host).await?,
        };
        let mut last = None;
        for address in addresses {
            match TcpStream::connect(SocketAddr::new(address, self.port)).await {
                Ok(stream) => return Ok(stream),
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("Proxy host does not resolve")))
    }
    pub(super) async fn open(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        let mut stream = self.connect().await?;
        match self.kind.as_str() {
            "socks4" | "socks4a" => {
                relay_protocol::socks4(&mut stream, host, port, self.user.as_deref()).await?
            }
            "socks" | "socks5" | "socks5h" => {
                relay_protocol::socks5(
                    &mut stream,
                    host,
                    port,
                    self.user.as_deref(),
                    self.password.as_deref(),
                )
                .await?
            }
            _ => {}
        }
        Ok(stream)
    }
}

pub struct Relay {
    address: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    completion: mpsc::Receiver<io::Result<()>>,
    worker: Option<JoinHandle<()>>,
    last_error: Arc<Mutex<Option<String>>>,
}
impl Relay {
    pub fn start(proxy: &Proxy) -> io::Result<Self> {
        Self::start_with_dns(proxy, || SystemDns::new(Duration::from_secs(15)))
    }
    fn start_with_dns(
        proxy: &Proxy,
        initialize: impl FnOnce() -> io::Result<SystemDns>,
    ) -> io::Result<Self> {
        let server = if proxy.server.contains("://") {
            proxy.server.clone()
        } else {
            format!("http://{}", proxy.server)
        };
        let url = Url::parse(&server).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bad upstream proxy address")
        })?;
        let kind = url.scheme().to_owned();
        if !matches!(
            kind.as_str(),
            "http" | "https" | "socks" | "socks5" | "socks5h" | "socks4" | "socks4a"
        ) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "the relay cannot forward to a '{kind}' proxy — it speaks HTTP, SOCKS4 and SOCKS5."
                ),
            ));
        }
        let record = proxy.to_record();
        let user = record["username"].as_str().map(str::to_owned);
        let password = record["password"].as_str().map(str::to_owned);
        if matches!(kind.as_str(), "socks4" | "socks4a")
            && password
                .as_ref()
                .is_some_and(|password| !password.is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "SOCKS4 has no password in the protocol, so a SOCKS4 proxy with a password cannot be used anywhere — use SOCKS5 for this one.",
            ));
        }
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "upstream proxy address needs a host and a port",
                )
            })?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let authority = server
            .split_once("://")
            .map_or("", |(_, tail)| tail)
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("");
        let authority = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let explicit_port = if let Some(bracketed) = authority.strip_prefix('[') {
            bracketed
                .split_once(']')
                .and_then(|(_, tail)| tail.strip_prefix(':'))
        } else {
            authority.rsplit_once(':').map(|(_, port)| port)
        };
        let port = explicit_port
            .and_then(|port| port.parse::<u16>().ok())
            .filter(|port| *port > 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "upstream proxy address needs a host and a port",
                )
            })?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        // Hickory construction is lazy; initialize before any worker/runtime exists.
        // A system-configuration error cannot orphan a thread or drop a runtime here.
        let dns = initialize()?;
        let (stop, mut stopped) = oneshot::channel();
        let (done, completion) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        let last_error = Arc::new(Mutex::new(None));
        let errors = last_error.clone();
        let worker = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = done.send(Err(error));
                    return;
                }
            };
            let result = runtime.block_on(async {
                let listener = TcpListener::from_std(listener)?;
                let upstream = Arc::new(Upstream { kind, host, port, user, password,
                    dns, last_error: errors });
                ready.send(()).map_err(|_| io::Error::other("Relay startup owner disappeared"))?;
                let mut tasks = JoinSet::new();
                let outcome = loop {
                    tokio::select! {
                        biased;
                        _ = &mut stopped => break Ok(()),
                        finished = tasks.join_next(), if !tasks.is_empty() => {
                            if finished.is_some_and(|result| result.is_err()) {
                                break Err(io::Error::other("Relay connection task failed"));
                            }
                        },
                        accepted = listener.accept(), if tasks.len() < 64 => {
                            match accepted {
                                Ok((client, _)) => {
                                    let upstream = upstream.clone();
                                    tasks.spawn(async move { relay_http::serve(client, upstream).await });
                                },
                                Err(error) => break Err(error),
                            }
                        }
                    }
                };
                drop(listener);
                tasks.shutdown().await;
                outcome
            });
            drop(runtime);
            let _ = done.send(result);
        });
        let mut relay = Self {
            address,
            stop: Some(stop),
            completion,
            worker: Some(worker),
            last_error,
        };
        if readiness.recv_timeout(Duration::from_secs(15)).is_err() {
            relay.close(Duration::from_secs(5))?;
            return Err(io::Error::other("Native relay did not become ready"));
        }
        Ok(relay)
    }
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|last| last.clone())
    }
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
    pub fn close(&mut self, budget: Duration) -> io::Result<()> {
        if self.worker.is_none() {
            return Ok(());
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let outcome = match self.completion.recv_timeout(budget) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Relay shutdown remains unverified; owner retained",
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::other(
                "Relay worker ended without shutdown acknowledgement",
            )),
        };
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("Relay worker panicked"))?;
        }
        outcome
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        if let Err(error) = self.close(Duration::from_secs(5)) {
            eprintln!("Native relay cleanup remains unverified: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_failure_on_entered_runtime_returns_error_before_worker_birth()
    -> io::Result<()> {
        let proxy = Proxy::from_record(
            &serde_json::json!({
                "server":"http://127.0.0.1:9","username":"fixture","password":null,"country":null
            }),
            false,
        )
        .map_err(io::Error::other)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let caller = thread::current().id();
        let (observed, observation) = mpsc::sync_channel(1);
        let result = runtime.block_on(async {
            Relay::start_with_dns(&proxy, move || {
                observed
                    .send(thread::current().id())
                    .map_err(io::Error::other)?;
                Err(io::Error::other("Fixture DNS initialization failure"))
            })
        });
        drop(runtime);
        let error = result
            .err()
            .ok_or_else(|| io::Error::other("Failed DNS initialized relay"))?;
        assert_eq!(error.to_string(), "Fixture DNS initialization failure");
        assert_eq!(
            caller,
            observation
                .recv_timeout(Duration::from_secs(1))
                .map_err(io::Error::other)?
        );
        Ok(())
    }

    #[test]
    fn failed_shutdown_retains_worker_until_a_bounded_retry_joins_it() -> io::Result<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let (stop, stopped) = oneshot::channel::<()>();
        let (done, completion) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let outcome = released
                .recv_timeout(Duration::from_secs(2))
                .map_err(io::Error::other);
            drop(listener);
            drop(stopped);
            let _ = done.send(outcome);
        });
        let mut relay = Relay {
            address,
            stop: Some(stop),
            completion,
            worker: Some(worker),
            last_error: Arc::new(Mutex::new(None)),
        };
        let error = relay
            .close(Duration::ZERO)
            .expect_err("Unreleased worker reported settled");
        let retained = relay.worker.is_some();
        release.send(()).map_err(io::Error::other)?;
        relay.close(Duration::from_secs(2))?;
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(retained, "Failed cleanup discarded worker");
        assert!(relay.worker.is_none());
        relay.close(Duration::from_secs(1))?;
        Ok(())
    }
}
