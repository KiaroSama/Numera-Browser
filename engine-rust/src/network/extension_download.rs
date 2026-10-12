use super::{client, get};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};
use std::{io, time::Duration};

const STORE: &str = "https://clients2.google.com/service/update2/crx";
const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0";
const MAX_PACKAGE: usize = 64 * 1024 * 1024;

pub fn url(id: &str) -> io::Result<Url> {
    let id = crate::storage::extensions::extension_id(id)?;
    Url::parse(&format!(
        "{STORE}?response=redirect&prodversion=131.0.0.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
    ))
    .map_err(|_| io::Error::other("Extension store URL is invalid"))
}

/// Native TLS, redirects and bounded streaming; extraction remains storage-owned.
pub async fn download(id: &str) -> io::Result<Vec<u8>> {
    let url = url(id)?;
    download_from(id, url, Duration::from_secs(60), MAX_PACKAGE).await
}

pub(crate) async fn download_from(
    id: &str,
    url: Url,
    budget: Duration,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(AGENT));
    let client = client(budget)?;
    let bytes = get(&client, url, headers, limit).await.map_err(|error| {
        io::Error::new(error.kind(), format!("could not download {id}: {error}"))
    })?;
    if !bytes.starts_with(b"Cr24") {
        return Err(io::Error::other(format!(
            "the store did not return an extension for id '{id}' - check the id is correct and the extension is still published."
        )));
    }
    Ok(bytes)
}

/// Synchronous service owns its runtime, including callers on blocking workers.
pub fn download_sync(id: &str) -> io::Result<Vec<u8>> {
    let run = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(download(id))
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(run)
                .join()
                .map_err(|_| io::Error::other("Native download worker panicked"))?
        })
    } else {
        run()
    }
}
