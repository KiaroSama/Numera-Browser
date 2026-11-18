//! Offline image browser acceptance. The native SDK implementation and fault
//! controls share this exact finite resource state machine.
use serde_json::Value;
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
};
pub const PAGE_TEXT: &str = "numera-container-ready";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Launch,
    Context,
    Page,
    Content,
    Text,
    Evaluate,
    ClosePage,
    CloseContext,
    CloseBrowser,
    ConnectionCheck,
    Stop,
}
pub trait Driver {
    fn installed_version(&self) -> io::Result<String>;
    fn non_root(&self) -> io::Result<bool>;
    fn executable(&self, root: &Path, selected: &Path) -> io::Result<PathBuf> {
        image_executable(root, selected)
    }
    fn perform(&mut self, phase: Phase) -> Pin<Box<dyn Future<Output = io::Result<Value>> + '_>>;
}
pub fn exact_version(version: &str) -> bool {
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}
pub fn image_executable(root: &Path, selected: &Path) -> io::Result<PathBuf> {
    let root = root.canonicalize()?;
    let selected = selected.canonicalize()?;
    if !selected.starts_with(root) || !selected.is_file() {
        return Err(io::Error::other(
            "Image Chromium executable is absent or outside its owned directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&selected)?.permissions().mode() & 0o111 == 0 {
            return Err(io::Error::other(
                "Image Chromium executable is not executable",
            ));
        }
    }
    Ok(selected)
}
pub async fn check(
    driver: &mut impl Driver,
    expected: &str,
    browser_root: &Path,
) -> io::Result<()> {
    if !exact_version(expected) {
        return Err(io::Error::other(
            "Image must declare an exact Playwright release",
        ));
    }
    if driver.installed_version()? != expected {
        return Err(io::Error::other(
            "Playwright client does not match image pin",
        ));
    }
    if !driver.non_root()? {
        return Err(io::Error::other(
            "Run this Linux image smoke as its configured non-root user",
        ));
    }
    let selected = driver.perform(Phase::Start).await?;
    let mut browser = false;
    let mut context = false;
    let mut page = false;
    let result = async {
        let selected = selected
            .as_str()
            .ok_or_else(|| io::Error::other("Image browser path missing"))?;
        driver.executable(browser_root, Path::new(selected))?;
        driver.perform(Phase::Launch).await?;
        browser = true;
        driver.perform(Phase::Context).await?;
        context = true;
        driver.perform(Phase::Page).await?;
        page = true;
        driver.perform(Phase::Content).await?;
        if driver.perform(Phase::Text).await? != PAGE_TEXT {
            return Err(io::Error::other(
                "Bundled Chromium returned unexpected DOM content",
            ));
        }
        if driver.perform(Phase::Evaluate).await? != 42 {
            return Err(io::Error::other(
                "Bundled Chromium returned unexpected script result",
            ));
        }
        Ok(())
    }
    .await;
    let mut cleanup = None;
    for (created, phase) in [
        (page, Phase::ClosePage),
        (context, Phase::CloseContext),
        (browser, Phase::CloseBrowser),
    ] {
        if created && driver.perform(phase).await.is_err() {
            cleanup = Some(io::Error::other("Image browser resource did not close"));
        }
    }
    if browser {
        match driver.perform(Phase::ConnectionCheck).await {
            Ok(Value::Bool(false)) => (),
            _ => cleanup = Some(io::Error::other("Image browser disconnect is unproved")),
        }
    }
    if driver.perform(Phase::Stop).await.is_err() {
        cleanup = Some(io::Error::other("Image browser driver did not stop"));
    }
    result?;
    if let Some(error) = cleanup {
        return Err(error);
    }
    Ok(())
}
/// Only controlled static diagnostics leave this entry; vendor errors can carry
/// credentials and filesystem paths. Logs use the normal native redacting sink.
pub async fn logged(
    driver: &mut impl Driver,
    expected: &str,
    browser_root: &Path,
    logs: &Path,
) -> i32 {
    let log = crate::runtime_log::RuntimeLog::new(logs, "container-smoke");
    log.event("INFO", "numera.container-smoke", "Smoke starting");
    let code = if check(driver, expected, browser_root).await.is_ok() {
        0
    } else {
        log.event(
            "ERROR",
            "numera.container-smoke",
            "Container browser smoke failed; inspect its controlled phase evidence",
        );
        1
    };
    if code == 0 {
        log.event("INFO", "numera.container-smoke", "Smoke finished exit=0");
    } else {
        log.event("INFO", "numera.container-smoke", "Smoke finished exit=1");
    }
    code
}
