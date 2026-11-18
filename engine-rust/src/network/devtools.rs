use reqwest::{Url, header::HeaderMap};
use serde_json::Value;
use std::{io, time::Duration};

pub fn info(port: u16, budget: Duration) -> io::Result<Value> {
    if port == 0 || budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DevTools port and deadline must be positive",
        ));
    }
    let run = || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let client = super::builder(Duration::from_secs(2))?
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(super::failure)?;
            let url = Url::parse(&format!("http://127.0.0.1:{port}/json/version"))
                .map_err(io::Error::other)?;
            tokio::time::timeout(budget, async {
                loop {
                    if let Ok(bytes) =
                        super::get(&client, url.clone(), HeaderMap::new(), 65536).await
                        && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                        && value.is_object()
                    {
                        return Ok::<Value, io::Error>(value);
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            })
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("DevTools didn't come up on port {port}"),
                )
            })?
        })
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(run)
                .join()
                .map_err(|_| io::Error::other("DevTools query worker panicked"))?
        })
    } else {
        run()
    }
}
pub fn snippets(port: u16, ws: &str) -> String {
    format!(
        r#"
Playwright (Python)
    from playwright.sync_api import sync_playwright
    pw = sync_playwright().start()
    browser = pw.chromium.connect_over_cdp("http://127.0.0.1:{port}")
    page = browser.contexts[0].pages[0]

Puppeteer (Node)
    const browser = await puppeteer.connect({{ browserURL: "http://127.0.0.1:{port}" }});
    const [page] = await (await browser.pages());

Selenium (Python)
    from selenium import webdriver
    opts = webdriver.ChromeOptions()
    opts.add_experimental_option("debuggerAddress", "127.0.0.1:{port}")
    driver = webdriver.Chrome(options=opts)

WebSocket endpoint
    {ws}
"#
    )
}
