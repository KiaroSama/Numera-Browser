//! Actual bundled Chromium acceptance for the explicitly invoked image probe.
use super::{
    driver::{Driver as NativeDriver, Installation},
    *,
};
use crate::browser::container_smoke::{self, Driver, Phase};
use playwright_rs::server::connection::ConnectionExt;
use playwright_rs::{Browser, BrowserContext, Page};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};
pub struct Native {
    installation: Option<Installation>,
    driver: Option<NativeDriver>,
    browser: Option<Browser>,
    context: Option<BrowserContext>,
    page: Option<Page>,
    executable: PathBuf,
}
impl Native {
    pub fn new() -> io::Result<Self> {
        let bytes = std::fs::read("/app/browsers/manifest.json")?;
        if bytes.len() > 65536 {
            return Err(io::Error::other("Image browser manifest exceeds its bound"));
        }
        let manifest: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        let relative = manifest["playwright"]["executable"]
            .as_str()
            .ok_or_else(|| io::Error::other("Image Chromium manifest missing"))?;
        let executable =
            crate::storage::archive_paths::member_path(Path::new("/app/browsers"), relative)?;
        Ok(Self {
            installation: Some(Installation::discover("playwright")?),
            driver: None,
            browser: None,
            context: None,
            page: None,
            executable,
        })
    }
}
impl Driver for Native {
    fn installed_version(&self) -> io::Result<String> {
        Ok(self
            .installation
            .as_ref()
            .ok_or_else(|| io::Error::other("Image SDK metadata no longer available"))?
            .version
            .clone())
    }
    fn non_root(&self) -> io::Result<bool> {
        #[cfg(target_os = "linux")]
        {
            let status = std::fs::read_to_string("/proc/self/status")?;
            let uid = status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|line| line.split_ascii_whitespace().next())
                .ok_or_else(|| io::Error::other("Image execution identity missing"))?
                .parse::<u32>()
                .map_err(io::Error::other)?;
            Ok(uid != 0)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Container smoke requires Linux",
            ))
        }
    }
    fn perform(&mut self, phase: Phase) -> Pin<Box<dyn Future<Output = io::Result<Value>> + '_>> {
        Box::pin(async move {
            let result=async{
            match phase {
                Phase::Start=>{let driver=NativeDriver::start(self.installation.take().ok_or_else(invalid)?).await?;self.driver=Some(driver);Ok(json!(self.executable))},
                Phase::Launch=>{let playwright=self.driver.as_ref().ok_or_else(invalid)?.playwright()?;let browser=playwright.chromium();let response:Value=browser.channel().send("launch",json!({"executablePath":self.executable,"headless":true,"timeout":10000,"handleSIGINT":false,"handleSIGTERM":false,"handleSIGHUP":false})).await?;let guid=response["browser"]["guid"].as_str().ok_or_else(invalid)?;self.browser=Some(browser.connection().wait_for_typed::<Browser>(guid).await?);Ok(Value::Null)},
                Phase::Context=>{self.context=Some(self.browser.as_ref().ok_or_else(invalid)?.new_context().await?);Ok(Value::Null)},
                Phase::Page=>{self.page=Some(self.context.as_ref().ok_or_else(invalid)?.new_page().await?);Ok(Value::Null)},
                Phase::Content=>{self.page.as_ref().ok_or_else(invalid)?.set_content(&format!("<main id=\"numera-smoke\">{}</main>",container_smoke::PAGE_TEXT),Some(GotoOptions::new().timeout(Duration::from_secs(5)).wait_until(WaitUntil::Load))).await?;Ok(Value::Null)},
                Phase::Text=>{Ok(json!(self.page.as_ref().ok_or_else(invalid)?.locator("#numera-smoke").inner_text().await?))},
                Phase::Evaluate=>{self.page.as_ref().ok_or_else(invalid)?.evaluate::<(),Value>("() => 6 * 7",None).await},
                Phase::ClosePage=>{if let Some(page)=self.page.take(){page.close().await?;}Ok(Value::Null)},
                Phase::CloseContext=>{if let Some(context)=self.context.take(){context.close().await?;}Ok(Value::Null)},
                Phase::CloseBrowser=>{if let Some(browser)=self.browser.as_ref(){browser.close().await?;}Ok(Value::Null)},
                Phase::ConnectionCheck=>{Ok(json!(self.browser.as_ref().is_some_and(Browser::is_connected)))},
                Phase::Stop=>{if let Some(driver)=self.driver.take(){driver.stop().await?;}self.browser.take();Ok(Value::Null)},
            }
        }.await;
            result.map_err(|_: playwright_rs::Error| {
                io::Error::other("Native image browser phase failed")
            })
        })
    }
}
pub fn main() -> ExitCode {
    let logs = std::env::var_os("NUMERA_SMOKE_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("numera-container-logs"));
    let expected = std::env::var("NUMERA_CONTAINER_PLAYWRIGHT_VERSION").unwrap_or_default();
    if !container_smoke::exact_version(&expected) {
        eprintln!("Image must declare an exact Playwright release");
        return ExitCode::FAILURE;
    }
    let root = Path::new("/app/browsers");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let code = match (runtime, Native::new()) {
        (Ok(runtime), Ok(mut native)) => {
            runtime.block_on(container_smoke::logged(&mut native, &expected, root, &logs))
        }
        _ => {
            eprintln!("Container browser smoke requires an exact provisioned SDK manifest");
            1
        }
    };
    ExitCode::from(code as u8)
}
