//! SDK-only subprocess in Rust. Application state, consent, leases and browser
//! selection remain in the native owner. Stdout contains bounded JSON frames.
mod camoufox;
pub mod camoufox_coherence;
pub mod camoufox_config;
pub mod camoufox_cpu;
pub mod camoufox_geo;
mod camoufox_host;
pub mod camoufox_identity;
pub mod camoufox_webgl;
pub mod cloak;
pub mod container_smoke;
mod driver;
pub mod fingerprint;
mod options;
mod policy;
#[cfg(feature = "test-support")]
mod testing;
pub mod values;

pub use options::persistent as persistent_options;

use super::adapter::{PROTOCOL, frame};
use crate::{native_input::Input, runtime_log::RuntimeLog};
use driver::{Driver, Installation};
use playwright_rs::protocol::MouseOptions;
use playwright_rs::server::channel_owner::ChannelOwner;
use playwright_rs::{BrowserContext, FulfillOptions, GotoOptions, Page, WaitUntil};
use serde_json::{Value, json};
use std::{
    io::{self, BufReader, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn invalid() -> playwright_rs::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Invalid SDK request").into()
}
fn error_category(error: &playwright_rs::Error) -> &'static str {
    use playwright_rs::Error;
    match error {
        Error::Io(error) => match error.kind() {
            io::ErrorKind::NotFound => "io-not-found",
            io::ErrorKind::PermissionDenied => "io-denied",
            io::ErrorKind::InvalidInput => "io-invalid-input",
            io::ErrorKind::InvalidData => "io-invalid-data",
            io::ErrorKind::TimedOut => "io-timeout",
            _ => "io",
        },
        Error::Json(_) => "json",
        Error::ProtocolError(message) => {
            // Vendor errors may contain arguments, credentials, environment or
            // page content. Only fixed categories may cross the log boundary.
            if message.contains("error while loading shared libraries") {
                "protocol-library"
            } else if message.contains("no DISPLAY environment variable")
                || message.contains("cannot open display")
            {
                "protocol-display"
            } else if message.contains("profile is in use") || message.contains("already running") {
                "protocol-profile-busy"
            } else if message.contains("MOZ_CRASH") || message.contains("signal=SIGSEGV") {
                "protocol-browser-crash"
            } else if message.contains("ValidationError") || message.contains("expected object") {
                "protocol-validation"
            } else if message.contains("Failed to launch") {
                "protocol-launch"
            } else if message.contains("Target page, context or browser has been closed") {
                "protocol-target-closed"
            } else if message.contains("<process did exit:") {
                "protocol-browser-exit"
            } else if message.contains("Connection closed") {
                "protocol-connection-closed"
            } else {
                "protocol"
            }
        }
        Error::TransportError(_) => "transport",
        Error::ServerError(_) => "server",
        Error::LaunchFailed(_) => "launch",
        Error::ConnectionFailed(_) => "connection",
        Error::TargetClosed { .. } => "target-closed",
        Error::ChannelClosed => "channel-closed",
        Error::UnknownObjectType(_) => "unknown-object",
        Error::ObjectNotFound(_) => "object-not-found",
        Error::TypeMismatch { .. } => "object-type",
        Error::Timeout(_) => "timeout",
        Error::InvalidArgument(_) => "invalid-argument",
        _ => "sdk",
    }
}
fn text<'a>(request: &'a Value, key: &str) -> playwright_rs::Result<&'a str> {
    request[key]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(invalid)
}
fn boolean(request: &Value, key: &str) -> playwright_rs::Result<bool> {
    request
        .get(key)
        .map_or(Ok(false), |value| value.as_bool().ok_or_else(invalid))
}

/// Read installed SDK metadata without creating an SDK process during updater
/// recovery. The browser-worker handshake uses the same installation checks.
pub fn metadata() -> Value {
    driver::metadata()
}

/// The browser family and persistent-context call sent to a provisioned vendor
/// driver. `options` has already passed the backend's native preparation.
#[derive(Debug)]
pub struct LaunchRequest {
    pub family: &'static str,
    pub method: &'static str,
    pub options: Value,
}

pub fn launch_request(
    backend: &str,
    options: &serde_json::Map<String, Value>,
) -> io::Result<LaunchRequest> {
    let family = match backend {
        "camoufox" => "firefox",
        "playwright" | "patchright" | "cloak" => "chromium",
        _ => return Err(io::Error::other("Unknown SDK backend")),
    };
    if options
        .get("userDataDir")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(io::Error::other("Persistent profile directory required"));
    }
    Ok(LaunchRequest {
        family,
        method: "launchPersistentContext",
        options: Value::Object(options.clone()),
    })
}

async fn evaluate_raw(
    page: &Page,
    expression: &str,
    argument: &Value,
    main_world: bool,
) -> playwright_rs::Result<Value> {
    let frame = page.main_frame().await?;
    let mut parameters = json!({"expression":expression,"arg":playwright_rs::protocol::serialize_argument(argument)});
    if main_world {
        parameters["isolatedContext"] = false.into();
    }
    let response: Value = frame
        .channel()
        .send("evaluateExpression", parameters)
        .await?;
    Ok(response["value"].clone())
}
fn operation(request: &Value) -> &'static str {
    match request["operation"].as_str() {
        Some("hello") => "hello",
        Some("open") => "open",
        Some("navigate") => "navigate",
        Some("mouse") => "mouse",
        Some("route") => "route",
        Some("evaluate") => "evaluate",
        Some("inspect") => "inspect",
        Some("cookies-read") => "cookies-read",
        Some("cookies-clear") => "cookies-clear",
        Some("cookies-add") => "cookies-add",
        Some("close") => "close",
        _ => "invalid",
    }
}
fn send(output: &mut impl Write, id: u64, result: Result<Value, &'static str>) -> io::Result<()> {
    let value = match result {
        Ok(value) => json!({"protocol":PROTOCOL,"id":id,"ok":true,"result":value}),
        Err(kind) => {
            json!({"protocol":PROTOCOL,"id":id,"ok":false,"error":{"kind":kind,"type":"NativeSdkError"}})
        }
    };
    let mut bytes = values::encode(&value)?;
    bytes.push(b'\n');
    output.write_all(&bytes)?;
    output.flush()
}

#[derive(Default)]
struct Worker {
    initialized: bool,
    driver: Option<Driver>,
    context: Option<BrowserContext>,
    page: Option<Page>,
    policy: Option<Arc<policy::Policy>>,
    patchright: bool,
    camoufox: Option<camoufox::Prepared>,
    cloak: Option<cloak::Prepared>,
}
impl Worker {
    async fn execute(
        &mut self,
        request: &Value,
        closed: &Arc<AtomicBool>,
    ) -> playwright_rs::Result<Value> {
        match operation(request) {
            "hello" if !self.initialized => {
                self.initialized = true;
                return Ok(driver::metadata());
            }
            "open" if self.initialized && self.context.is_none() => {
                let backend = text(request, "backend")?;
                if !matches!(backend, "playwright" | "patchright" | "camoufox" | "cloak") {
                    return Err(invalid());
                }
                self.patchright = backend == "patchright";
                let options = request["options"].as_object().ok_or_else(invalid)?;
                let scripts = request
                    .get("init_scripts")
                    .map_or(Ok(Vec::new()), |scripts| {
                        scripts
                            .as_array()
                            .ok_or_else(invalid)?
                            .iter()
                            .map(|script| script.as_str().ok_or_else(invalid))
                            .collect::<playwright_rs::Result<Vec<_>>>()
                    })?;
                let policy = request
                    .get("cdp_policy")
                    .filter(|value| !value.is_null())
                    .map(policy::Policy::parse)
                    .transpose()?;
                if backend == "camoufox" && (policy.is_some() || !scripts.is_empty()) {
                    return Err(invalid());
                }
                // Validate the selected binary and prepare its launch before
                // starting vendor code. A refused Cloak offer must not create
                // a driver process or a browser profile as a side effect.
                let prepared = match backend {
                    "cloak" => {
                        self.cloak = Some(cloak::prepare_async(options).await?);
                        Some(self.cloak.as_ref().ok_or_else(invalid)?.options.clone())
                    }
                    "camoufox" => {
                        camoufox::binary(options)?;
                        None
                    }
                    _ => Some(options::persistent(options)?),
                };
                self.driver = Some(Driver::start(Installation::discover(backend)?).await?);
                let driver = self.driver.as_ref().ok_or_else(invalid)?;
                let options = match backend {
                    "camoufox" => {
                        self.camoufox = Some(camoufox::Prepared::default());
                        self.camoufox
                            .as_mut()
                            .ok_or_else(invalid)?
                            .prepare(options, driver)
                            .await?
                    }
                    _ => prepared.ok_or_else(invalid)?,
                };
                let context = match driver.open(backend, options).await {
                    Ok(context) => context,
                    Err(error) => {
                        if let Some(denial) = self
                            .cloak
                            .as_ref()
                            .and_then(|cloak| cloak.launch_error(&error.to_string()))
                        {
                            return Err(denial.into_error().into());
                        }
                        return Err(error);
                    }
                };
                self.context = Some(context.clone());
                let signal = Arc::clone(closed);
                context
                    .on_close(move || {
                        signal.store(true, Ordering::Release);
                        async { Ok(()) }
                    })
                    .await?;
                for script in scripts {
                    context.add_init_script(script).await?;
                }
                if let Some(policy) = &policy {
                    policy.apply(&context).await?;
                }
                let page = match context.pages().into_iter().next() {
                    Some(page) => page,
                    None => context.new_page().await?,
                };
                if std::env::var("NUMERA_ADAPTER_FIRST_PAGE").as_deref() == Ok("1") {
                    let signal = Arc::clone(closed);
                    page.on_close(move || {
                        signal.store(true, Ordering::Release);
                        async { Ok(()) }
                    })
                    .await?;
                }
                if let Some(policy) = &policy {
                    policy.ready(&context, &page).await?;
                }
                self.page = Some(page);
                self.policy = policy;
                return Ok(json!({"opened":true,"backend":backend}));
            }
            _ => {}
        }
        if !self.initialized {
            return Err(invalid());
        }
        let context = self.context.as_ref().ok_or_else(invalid)?;
        let page = self.page.as_ref().ok_or_else(invalid)?;
        match operation(request) {
            "navigate" => {
                let url = text(request, "url")?;
                let new_page = boolean(request, "new_page")?;
                let tolerate = boolean(request, "tolerate_failure")?;
                let timeout = request
                    .get("timeout_ms")
                    .map_or(Some(30000), Value::as_u64)
                    .filter(|timeout| (1..=45000).contains(timeout))
                    .ok_or_else(invalid)?;
                if new_page {
                    let page = context.new_page().await?;
                    if let Some(policy) = &self.policy {
                        policy.ready(context, &page).await?;
                    }
                    self.page = Some(page);
                }
                let page = self.page.as_ref().ok_or_else(invalid)?;
                let result = page
                    .goto(
                        url,
                        Some(
                            GotoOptions::new()
                                .wait_until(WaitUntil::DomContentLoaded)
                                .timeout(Duration::from_millis(timeout)),
                        ),
                    )
                    .await;
                if !tolerate {
                    result.as_ref().map_err(|_| invalid())?;
                }
                Ok(json!({"navigated":result.is_ok()}))
            }
            "mouse" => {
                let x = request["x"]
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(invalid)?;
                let y = request["y"]
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(invalid)?;
                let steps = request
                    .get("steps")
                    .map_or(Some(1), Value::as_u64)
                    .filter(|steps| (1..=1000).contains(steps))
                    .ok_or_else(invalid)?;
                let result = match text(request, "action")? {
                    "move" => {
                        page.mouse()
                            .move_to(
                                x,
                                y,
                                Some(MouseOptions::builder().steps(steps as u32).build()),
                            )
                            .await
                    }
                    "wheel" => page.mouse().wheel(x, y).await,
                    _ => return Err(invalid()),
                };
                Ok(json!({"performed":result.is_ok()}))
            }
            "route" => {
                let pattern = text(request, "pattern")?;
                let body = request["body"]
                    .as_str()
                    .ok_or_else(invalid)?
                    .as_bytes()
                    .to_vec();
                let content_type = text(request, "content_type")?.to_owned();
                let status = request["status"]
                    .as_u64()
                    .filter(|status| (100..=599).contains(status))
                    .ok_or_else(invalid)? as u16;
                page.route(pattern, move |route| {
                    let options = FulfillOptions::builder()
                        .status(status)
                        .body(body.clone())
                        .content_type(content_type.clone())
                        .build();
                    async move { route.fulfill(options).await }
                })
                .await?;
                Ok(json!({"routed":true}))
            }
            "evaluate" => {
                let expression = text(request, "expression")?;
                let tolerate = boolean(request, "tolerate_failure")?;
                let result = evaluate_raw(page, expression, &request["argument"], false).await;
                match result {
                    Ok(value) => Ok(json!({"evaluated":true,"value":values::decode(&value)?})),
                    Err(_) if tolerate => Ok(json!({"evaluated":false,"value":null})),
                    Err(error) => Err(error),
                }
            }
            "inspect" => {
                let expression = crate::page_runtime::expression(
                    crate::page_runtime::Program::InspectIdentity,
                );
                // Patchright identity probes must observe the page's main world.
                let raw = evaluate_raw(page, &expression, &Value::Null, self.patchright).await?;
                Ok(values::decode(&raw)?)
            }
            // Preserve vendor cookie fields (including partition keys) instead
            // of projecting them through the binding's smaller Cookie struct.
            "cookies-read" => context.channel().send("cookies", json!({"urls":[]})).await,
            "cookies-clear" => {
                context
                    .channel()
                    .send_no_result("clearCookies", json!({}))
                    .await?;
                Ok(json!({"cleared":true}))
            }
            "cookies-add" => {
                let rows = request["cookies"].as_array().ok_or_else(invalid)?;
                if rows.iter().any(|row| !row.is_object()) {
                    return Err(invalid());
                }
                context
                    .channel()
                    .send_no_result("addCookies", json!({"cookies":rows}))
                    .await?;
                Ok(json!({"added":true}))
            }
            _ => Err(invalid()),
        }
    }
    async fn cleanup(&mut self) -> bool {
        let mut success = true;
        if let Some(context) = self.context.take() {
            success &= matches!(
                tokio::time::timeout(Duration::from_secs(10), context.close()).await,
                Ok(Ok(()))
            );
        }
        self.policy.take();
        self.page.take();
        if let Some(camoufox) = self.camoufox.as_mut() {
            success &= camoufox.cleanup_probe().await;
        }
        if let Some(driver) = self.driver.take() {
            success &= driver.stop().await.is_ok();
        }
        self.camoufox.take();
        self.cloak.take();
        success
    }
}

async fn run() -> io::Result<()> {
    let directory = std::env::var_os("NUMERA_ADAPTER_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("logs"));
    let log = RuntimeLog::new(&directory, "browser-adapter");
    log.event(
        "INFO",
        "numera.browser-adapter",
        "Starting native SDK protocol=1",
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicBool::new(false));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let cancel = Arc::clone(&cancelled);
    #[cfg(feature = "test-support")]
    let schedule = testing::Schedule::from_environment()?;
    #[cfg(feature = "test-support")]
    let input_schedule = Arc::clone(&schedule);
    let input = std::thread::spawn(move || {
        #[cfg(feature = "test-support")]
        let input = Input::with_wait(io::stdin(), cancel, input_schedule.wait())
            .expect("Test scheduling wait is bounded")
            .with_observer(input_schedule);
        #[cfg(not(feature = "test-support"))]
        let input = Input::new(io::stdin(), cancel);
        let mut reader = BufReader::new(input);
        loop {
            let result = frame(&mut reader);
            let done = !matches!(result, Ok(Some(_)));
            if sender.blocking_send(result).is_err() || done {
                break;
            }
        }
    });
    let mut output = io::stdout().lock();
    let mut worker = Worker::default();
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut closing = None;
    let mut close_started = None;
    let mut failed = None;
    let result = async {
        loop {
            if closed.load(Ordering::Acquire) { break; }
            let request = tokio::select! {
                request = receiver.recv() => match request { Some(request) => request?, None => break },
                _ = tokio::time::sleep(Duration::from_millis(25)) => {
                    #[cfg(feature = "test-support")]
                    schedule.idle(worker.context.is_some());
                    if worker.context.is_none() && Instant::now() >= deadline {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "SDK input deadline exceeded"));
                    }
                    continue;
                }
            };
            let Some(request) = request else { break; };
            let id = request["id"].as_u64().filter(|id| *id > 0)
                .filter(|_| request["protocol"].as_u64() == Some(PROTOCOL))
                .ok_or_else(|| io::Error::other("Invalid SDK protocol envelope"))?;
            let label = operation(&request);
            if label == "close" && worker.initialized { closing = Some(id); close_started = Some(Instant::now()); break; }
            let start = Instant::now();
            let mut result = worker.execute(&request, &closed).await;
            if let Some(denial) = worker.cloak.as_ref().and_then(cloak::Prepared::denial) {
                result = Err(denial.into_error().into());
            }
            // SDK errors can embed credentials, request URLs and source text.
            // Only an allowlisted operation and the native error category leave
            // this boundary; never format the vendor error itself.
            log.message("INFO", "numera.browser-adapter", &format!("SDK timing operation={label} elapsed_ns={}", start.elapsed().as_nanos()));
            match result {
                Ok(value) => {
                    if label == "open" { log.event("INFO", "numera.browser-adapter", "Persistent context opened"); }
                    if let Err(error) = send(&mut output, id, Ok(value)) {
                        if error.kind() == io::ErrorKind::InvalidData {
                            log.event("ERROR", "numera.browser-adapter", "SDK response encoding failed");
                            send(&mut output, id, Err("sdk"))?;
                        }
                        return Err(error);
                    }
                }
                Err(error) => {
                    if label == "open" && let Some(camoufox) = &worker.camoufox {
                        log.message("ERROR", "numera.browser-adapter", &format!("Camoufox preparation failed phase={}", camoufox.phase));
                    }
                    log.message("ERROR", "numera.browser-adapter", &format!("SDK operation failed category={}",error_category(&error)));
                    // Report cleanup separately only after observing the
                    // native guards. Vendor error text cannot set this flag.
                    failed = Some(id);
                    return Err(io::Error::other("SDK operation failed"));
                }
            }
        }
        Ok(())
    }.await;
    cancelled.store(true, Ordering::Release);
    receiver.close();
    drop(receiver);
    let input_ok = input.join().is_ok();
    let cleanup_started = Instant::now();
    let cleanup_ok = worker.cleanup().await && input_ok;
    if let Some(started) = close_started {
        log.message(
            "INFO",
            "numera.browser-adapter",
            &format!(
                "SDK timing operation=close elapsed_ns={}",
                started.elapsed().as_nanos()
            ),
        );
    }
    log.message(
        "INFO",
        "numera.browser-adapter",
        &format!(
            "SDK timing operation=cleanup elapsed_ns={}",
            cleanup_started.elapsed().as_nanos()
        ),
    );
    if let Some(id) = failed {
        send(
            &mut output,
            id,
            Err(if cleanup_ok { "sdk" } else { "sdk-cleanup" }),
        )?;
    }
    if let Some(id) = closing {
        send(
            &mut output,
            id,
            if cleanup_ok {
                Ok(json!({"closing":true}))
            } else {
                Err("cleanup")
            },
        )?;
    }
    log.event(
        "INFO",
        "numera.browser-adapter",
        if result.is_ok() && cleanup_ok {
            "Finished exit=0"
        } else {
            "Finished exit=1"
        },
    );
    result?;
    if !cleanup_ok {
        return Err(io::Error::other("SDK cleanup failed"));
    }
    Ok(())
}

pub fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() == 1 && arguments[0] == "--build-info" {
        let result = crate::native_build::identity().and_then(|mut identity| {
            identity["component"] = "browser-runtime".into();
            serde_json::to_writer(io::stdout().lock(), &identity).map_err(io::Error::other)
        });
        return if result.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if arguments.len() == 1 && arguments[0] == "--metadata" {
        return if serde_json::to_writer(io::stdout().lock(), &driver::metadata()).is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if !arguments.is_empty() {
        return ExitCode::FAILURE;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    match runtime {
        Ok(runtime) if runtime.block_on(run()).is_ok() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
