//! Desktop window, owned retained-Python engine and consent-gated portable updates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod lifecycle;
use lifecycle::{Engine, Log};
use numera_desktop::updates::{self, bridge::Bridge};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tauri::{Manager, RunEvent};

struct DesktopUpdates(Mutex<Option<Bridge>>);
struct RuntimeLease(Mutex<Option<std::fs::File>>);
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--build-info") {
        let identity = serde_json::json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser","version":env!("CARGO_PKG_VERSION"),"source_sha":env!("NUMERA_BUILD_SOURCE_SHA"),"generation":env!("NUMERA_BUILD_GENERATION").parse::<u64>().unwrap_or(0)});
        #[cfg(feature = "update-acceptance")]
        let identity = {
            let mut value = identity;
            value["acceptance"] = true.into();
            value
        };
        println!("{identity}");
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--numera-update-recover") {
        let result = std::env::args_os()
            .nth(2)
            .ok_or_else(|| std::io::Error::other("Recovery journal argument missing"))
            .and_then(|path| updates::recovery::rollback(std::path::Path::new(&path)));
        if result.is_err() {
            eprintln!(
                "Update recovery requires attention; authenticated files and journal retained"
            );
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--numera-update-worker") {
        let result = std::env::args_os()
            .nth(2)
            .ok_or_else(|| std::io::Error::other("Update journal argument missing"))
            .and_then(|path| updates::worker::run(std::path::Path::new(&path)));
        if let Err(error) = result {
            eprintln!("Update recovery remains retained: {error}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::var_os("NUMERA_UPDATE_RECOVERY").is_none() {
        if let Ok(executable) = std::env::current_exe() {
            if let Some(root) = executable.parent() {
                match updates::recovery::pending(root) {
                    Ok(Some(_)) => {
                        eprintln!("An interrupted update is retained. Run the retained update-worker with --numera-update-recover and its journal before starting this installation.");
                        std::process::exit(1);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        eprintln!("Update recovery metadata is uncertain; startup refused without changing files.");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
    let context = tauri::generate_context!();
    #[cfg(feature = "update-acceptance")]
    let context = {
        let mut context = context;
        for window in &mut context.config_mut().app.windows {
            window.visible = false;
            window.focus = false;
        }
        context
    };
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let log: Log = Arc::new(Mutex::new(VecDeque::new()));
            let token = updates::files::nonce()?;
            let root = std::env::current_exe()?
                .parent()
                .ok_or("Desktop directory missing")?
                .to_owned();
            let portable = root.join("BUILD.sig").is_file();
            if portable {
                app.manage(RuntimeLease(Mutex::new(Some(
                    updates::install_lease::shared(&root)?,
                ))));
            }
            match lifecycle::start_engine(log.clone(), &token) {
                Ok((mut child, port, reader)) => {
                    #[cfg(windows)]
                    {
                        let owner = lifecycle::job::Job::new()
                            .ok_or("Could not own the engine process tree")?;
                        if !owner.adopt(&child) {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err("Engine process ownership refused".into());
                        }
                        app.manage(owner);
                    }
                    app.manage(Engine(Mutex::new(Some(child))));
                    app.manage(lifecycle::Reader(Mutex::new(reader)));
                    let root = std::env::current_exe()?
                        .parent()
                        .ok_or("Desktop directory missing")?
                        .to_owned();
                    let application = handle.clone();
                    let service = updates::Service::new(
                        root.clone(),
                        port,
                        token.clone(),
                        Arc::new(move || application.exit(0)),
                    )?;
                    let bridge = Bridge::start(service, port)?;
                    app.manage(DesktopUpdates(Mutex::new(Some(bridge))));
                    std::thread::spawn(move || match lifecycle::wait_healthy(&handle, port) {
                        Ok(()) => match updates::worker::recovery_ready(&root, port, &token) {
                            Ok(()) => {
                                if let Some(updates) = handle.try_state::<DesktopUpdates>() {
                                    if let Ok(owner) = updates.0.lock() {
                                        if let Some(bridge) = owner.as_ref() {
                                            #[cfg(feature = "update-acceptance")]
                                            if bridge.report_acceptance(&root, port).is_err()
                                                || updates::acceptance::report_ready(&root).is_err()
                                            {
                                                handle.exit(1);
                                                return;
                                            }
                                            bridge.ready();
                                        }
                                    }
                                }
                                show_dashboard(&handle, port);
                            }
                            Err(error) => {
                                show_error(&handle, &format!("Update recovery is blocked: {error}"))
                            }
                        },
                        Err(why) => {
                            show_error(&handle, &format!("{why}\n\n{}", lifecycle::tail(&log)))
                        }
                    });
                }
                Err(why) => show_error(&handle, &why),
            }
            Ok(())
        })
        .on_page_load(|window, payload| {
            if window.label() != "main" {
                return;
            }
            if let Some(updates) = window.app_handle().try_state::<DesktopUpdates>() {
                if let Ok(bridge) = updates.0.lock() {
                    if let Some(bridge) = bridge.as_ref() {
                        if payload.url().origin().ascii_serialization() == bridge.origin() {
                            let _ = window.eval(bridge.injection());
                            #[cfg(feature = "update-acceptance")]
                            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                                if let Ok(executable) = std::env::current_exe() {
                                    if let Some(root) = executable.parent() {
                                        let root = root.to_owned();
                                        let handle = window.app_handle().clone();
                                        std::thread::spawn(move || {
                                            if updates::acceptance::report_webview(&root).is_err() {
                                                handle.exit(1);
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
        .build(context)
        .expect("failed to build the Numera Browser window")
        .run(|handle, event| {
            if matches!(event, RunEvent::Exit) {
                // Stop and reap the engine before releasing the activation
                // worker pipe; the update must never race a live old engine.
                if let Some(engine) = handle.try_state::<Engine>() {
                    if let Ok(mut owner) = engine.0.lock() {
                        if let Some(mut child) = owner.take() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                }
                if let Some(reader) = handle.try_state::<lifecycle::Reader>() {
                    if let Ok(mut owner) = reader.0.lock() {
                        if let Some(thread) = owner.take() {
                            let _ = thread.join();
                        }
                    }
                }
                if let Some(lease) = handle.try_state::<RuntimeLease>() {
                    if let Ok(mut owner) = lease.0.lock() {
                        owner.take();
                    }
                }
                if let Some(updates) = handle.try_state::<DesktopUpdates>() {
                    if let Ok(mut owner) = updates.0.lock() {
                        if let Some(mut bridge) = owner.take() {
                            bridge.stop();
                        }
                    }
                }
            }
        });
}
fn show_dashboard(handle: &tauri::AppHandle, port: u16) {
    let url = format!("http://127.0.0.1:{port}/");
    match (handle.get_webview_window("main"), tauri::Url::parse(&url)) {
        (Some(window), Ok(parsed)) => {
            if let Err(error) = window.navigate(parsed) {
                show_error(handle, &format!("Could not open dashboard: {error}"));
            }
        }
        _ => show_error(handle, "Could not open the dashboard."),
    }
}
fn show_error(handle: &tauri::AppHandle, message: &str) {
    if let Some(window) = handle.get_webview_window("main") {
        if let Ok(message) = serde_json::to_string(message) {
            let _ = window.eval(format!("window.numeraError({message})"));
        }
    }
}
