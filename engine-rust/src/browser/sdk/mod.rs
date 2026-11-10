//! SDK-only subprocess in Rust. Application state, consent, leases and browser
//! selection remain in the native owner. Stdout contains bounded JSON frames.
mod driver;
pub mod fingerprint;
mod options;
mod policy;

use super::adapter::{MAX_FRAME, PROTOCOL, frame};
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
    let mut bytes = serde_json::to_vec(&value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SDK response exceeds limit",
        ));
    }
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
                if !matches!(backend, "playwright" | "patchright") {
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
                let options = options::persistent(options)?;
                self.driver = Some(Driver::start(Installation::discover(backend)?).await?);
                let driver = self.driver.as_ref().ok_or_else(invalid)?;
                let context = driver.open(false, options).await?;
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
                let result = page
                    .evaluate::<Value, Value>(expression, Some(&request["argument"]))
                    .await;
                match result {
                    Ok(value) => Ok(json!({"evaluated":true,"value":value})),
                    Err(_) if tolerate => Ok(json!({"evaluated":false,"value":null})),
                    Err(error) => Err(error),
                }
            }
            "inspect" => {
                let expression = "async () => ({userAgent:navigator.userAgent,platform:navigator.platform,locale:navigator.language,languages:Array.from(navigator.languages),cores:navigator.hardwareConcurrency,width:innerWidth,height:innerHeight,userAgentData:navigator.userAgentData?await navigator.userAgentData.getHighEntropyValues(['platformVersion','architecture','bitness','model','uaFullVersion','fullVersionList','wow64']):null})";
                if self.patchright {
                    // Patchright evaluates in its utility world by default;
                    // identity inspection must observe the page's main world.
                    let frame = page.main_frame().await?;
                    let response: Value = frame.channel().send("evaluateExpression", json!({
                        "expression":expression, "arg":playwright_rs::protocol::serialize_null(),
                        "isolatedContext":false,
                    })).await?;
                    Ok(playwright_rs::protocol::parse_result(&response["value"]))
                } else {
                    page.evaluate::<Value, Value>(expression, None).await
                }
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
        if let Some(driver) = self.driver.take() {
            success &= driver.stop().await.is_ok();
        }
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
    let input = std::thread::spawn(move || {
        let mut reader = BufReader::new(Input::new(io::stdin(), cancel));
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
    let result = async {
        loop {
            if closed.load(Ordering::Acquire) { break; }
            let request = tokio::select! {
                request = receiver.recv() => match request { Some(request) => request?, None => break },
                _ = tokio::time::sleep(Duration::from_millis(25)) => {
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
            if label == "close" && worker.initialized { closing = Some(id); break; }
            let start = Instant::now();
            let result = worker.execute(&request, &closed).await;
            // SDK errors can embed credentials, request URLs and source text.
            // Only an allowlisted operation and the native error category leave
            // this boundary; never format the vendor error itself.
            log.message("INFO", "numera.browser-adapter", &format!("SDK timing operation={label} elapsed_ns={}", start.elapsed().as_nanos()));
            match result {
                Ok(value) => {
                    if label == "open" { log.event("INFO", "numera.browser-adapter", "Persistent context opened"); }
                    send(&mut output, id, Ok(value))?;
                }
                Err(_) => {
                    log.event("ERROR", "numera.browser-adapter", "SDK operation failed");
                    send(&mut output, id, Err("sdk"))?;
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
    log.message(
        "INFO",
        "numera.browser-adapter",
        &format!(
            "SDK timing operation=cleanup elapsed_ns={}",
            cleanup_started.elapsed().as_nanos()
        ),
    );
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
