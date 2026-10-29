use super::{
    application::{self, SessionApplication},
    scheduler::Scheduler,
};
use crate::{
    browser::runtime::Runtime,
    ownership::registry::{Registry, retry_retained_background},
};
use std::{
    collections::BTreeMap,
    io,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

pub struct Options {
    pub root: PathBuf,
    pub address: SocketAddr,
    pub token: String,
    pub origins: String,
    pub insecure_override: String,
    pub password: Option<String>,
    pub host: String,
    pub installed: BTreeMap<String, bool>,
    pub browser: Runtime,
    pub dashboard: Option<PathBuf>,
}
static RETAINED: OnceLock<Mutex<Vec<Server>>> = OnceLock::new();
struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;

/// The runtime, scheduler and registry remain owned until the worker is joined.
/// Readiness and stop timeouts leave this handle usable for a cleanup retry.
pub struct Server {
    address: SocketAddr,
    worker: Option<JoinHandle<io::Result<()>>>,
    ready: Option<Receiver<io::Result<()>>>,
    stop: Arc<AtomicBool>,
    shutdown: Option<oneshot::Sender<()>>,
    sessions: Arc<Registry>,
    cleaning: bool,
    failure: Option<io::Error>,
    log: Arc<crate::runtime_log::RuntimeLog>,
    settled: bool,
}
impl Server {
    pub fn start(options: Options) -> io::Result<Self> {
        let startup_admission = crate::ownership::update_admission::Reservation::begin(
            &options.root,
            "native-server-startup",
        )?;
        if let Some(message) = super::bind_policy::refusal(
            &options.address.ip().to_string(),
            &super::tokens::TokenStore::new(options.root.clone()),
            &options.token,
            &options.insecure_override,
        ) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, message));
        }
        let sessions = Arc::new(Registry::new(
            options.root.clone(),
            options.host.clone(),
            options.password.clone(),
        )?);
        let scheduler = Scheduler::new(
            options.root.clone(),
            options.password.clone(),
            options.host.clone(),
            sessions.clone(),
            options.browser.clone(),
        )?;
        let listener = std::net::TcpListener::bind(options.address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let log = Arc::new(crate::runtime_log::RuntimeLog::new(
            &options.browser.logs,
            "native-http",
        ));
        log.event("INFO", "server", "Startup requested");
        let worker_log = log.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_sessions = sessions.clone();
        let (shutdown, signal) = oneshot::channel();
        let (ready, receive) = std::sync::mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("numera-http".into())
            .spawn(move || {
                let mut startup_admission = startup_admission;
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready.send(Err(io::Error::new(error.kind(), error.to_string())));
                        return Err(error);
                    }
                };
                let _stop_on_exit = StopOnDrop(worker_stop.clone());
                runtime.block_on(async move {
                    let prepared = async {
                        let listener = tokio::net::TcpListener::from_std(listener)?;
                        let router = application::router_with_dashboard(
                            options.root,
                            &options.token,
                            &options.origins,
                            options.password,
                            &options.host,
                            SessionApplication {
                                installed: options.installed,
                                sessions: worker_sessions,
                            },
                            (Some(options.browser), options.dashboard.as_deref()),
                        )
                        .await?;
                        Ok::<_, io::Error>((listener, router))
                    }
                    .await;
                    let (listener, router) = match prepared {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            let _ =
                                ready.send(Err(io::Error::new(error.kind(), error.to_string())));
                            return Err(error);
                        }
                    };
                    startup_admission.release()?;
                    if worker_stop.load(Ordering::Acquire) || ready.send(Ok(())).is_err() {
                        return Ok(());
                    }
                    worker_log.event("INFO", "server", "Readiness published");
                    let scheduler_log = worker_log.clone();
                    let scheduler_stop = worker_stop.clone();
                    let scheduled = tokio::task::spawn_blocking(move || {
                        while !scheduler_stop.load(Ordering::Acquire) {
                            if let Err(error) = scheduler.tick() {
                                scheduler_log.message(
                                    "ERROR",
                                    "scheduler",
                                    &format!("Scheduler tick failed: {error}"),
                                );
                            }
                            for error in retry_retained_background(Duration::ZERO) {
                                eprintln!("Retained browser cleanup pending: {error}");
                            }
                            let deadline = Instant::now() + Duration::from_secs(60);
                            while Instant::now() < deadline
                                && !scheduler_stop.load(Ordering::Acquire)
                            {
                                thread::park_timeout(
                                    deadline
                                        .saturating_duration_since(Instant::now())
                                        .min(Duration::from_millis(50)),
                                );
                            }
                        }
                    });
                    let result = axum::serve(listener, router)
                        .with_graceful_shutdown(async {
                            let _ = signal.await;
                        })
                        .await;
                    worker_stop.store(true, Ordering::Release);
                    scheduled
                        .await
                        .map_err(|_| io::Error::other("Scheduler worker failed"))?;
                    result
                })
            })?;
        Ok(Self {
            address,
            worker: Some(worker),
            ready: Some(receive),
            stop,
            shutdown: Some(shutdown),
            sessions,
            cleaning: false,
            failure: None,
            log,
            settled: false,
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn wait_ready(&mut self, budget: Duration) -> io::Result<()> {
        if self.stop.load(Ordering::Acquire)
            || self.worker.as_ref().is_none_or(JoinHandle::is_finished)
        {
            return Err(io::Error::other("HTTP worker is not running"));
        }
        let Some(ready) = &self.ready else {
            return Ok(());
        };
        match ready.recv_timeout(budget) {
            Ok(Ok(())) => {
                self.ready = None;
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP readiness timed out; its owner is retained",
            )),
            Err(RecvTimeoutError::Disconnected) => {
                Err(io::Error::other("HTTP worker exited before readiness"))
            }
        }
    }
    pub fn stop(&mut self, budget: Duration) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let deadline = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("HTTP shutdown deadline invalid"))?;
        if self.settled {
            return Ok(());
        }
        loop {
            while self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
            {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "HTTP worker is still settling; its owner is retained",
                    ));
                }
                thread::park_timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(10)),
                );
            }
            if let Some(worker) = self.worker.take() {
                let result = worker
                    .join()
                    .unwrap_or_else(|_| Err(io::Error::other("HTTP worker panicked")));
                if self.cleaning {
                    self.cleaning = false;
                    if let Err(error) = result {
                        self.log
                            .event("ERROR", "server", "Cleanup remains unsettled");
                        return Err(error);
                    }
                    self.settled = true;
                    self.log.event("INFO", "server", "Shutdown settled");
                    return self.failure.take().map_or(Ok(()), Err);
                }
                self.failure = result.err();
            }
            let sessions = self.sessions.clone();
            self.worker = Some(
                thread::Builder::new()
                    .name("numera-http-cleanup".into())
                    .spawn(move || {
                        let mut failures = sessions
                            .shutdown(Duration::from_secs(10))
                            .into_iter()
                            .map(|(_, error)| error)
                            .collect::<Vec<_>>();
                        failures.extend(retry_retained_background(Duration::from_secs(10)));
                        if failures.is_empty() {
                            Ok(())
                        } else {
                            Err(io::Error::other(format!(
                                "HTTP browser cleanup remains unsettled: {}",
                                failures.join("; ")
                            )))
                        }
                    })?,
            );
            self.cleaning = true;
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        if let Err(error) = self.stop(Duration::from_secs(2)) {
            eprintln!("HTTP cleanup remains unsettled: {error}");
            if self.settled {
                return;
            }
            let retained = Self {
                address: self.address,
                worker: self.worker.take(),
                ready: self.ready.take(),
                stop: self.stop.clone(),
                shutdown: self.shutdown.take(),
                sessions: self.sessions.clone(),
                cleaning: self.cleaning,
                failure: self.failure.take(),
                log: self.log.clone(),
                settled: false,
            };
            self.settled = true;
            RETAINED
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(retained);
        }
    }
}
pub fn retry_retained(budget: Duration) -> Vec<String> {
    let retained = RETAINED.get_or_init(|| Mutex::new(Vec::new()));
    let owners = std::mem::take(
        &mut *retained
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let deadline = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut pending = Vec::new();
    let mut errors = Vec::new();
    for mut owner in owners {
        if let Err(error) = owner.stop(deadline.saturating_duration_since(Instant::now())) {
            errors.push(error.to_string());
            if !owner.settled {
                pending.push(owner);
            }
        }
    }
    retained
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    errors
}
