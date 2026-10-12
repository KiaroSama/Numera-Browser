//! Native bounded HTTP; SDK adapters never own application downloads or IP policy.
use reqwest::{Client, Url, header::HeaderMap};
use std::{io, sync::Arc, time::Duration};
mod dns;
pub mod relay;
mod relay_http;
mod relay_protocol;

pub(crate) fn failure(error: reqwest::Error) -> io::Error {
    let kind = if error.is_timeout() {
        io::ErrorKind::TimedOut
    } else {
        io::ErrorKind::Other
    };
    // URLs can carry caller credentials or rotation tokens.
    io::Error::new(kind, error.without_url().to_string())
}

pub fn client(budget: Duration) -> io::Result<Client> {
    builder(budget)?.build().map_err(failure)
}

pub(crate) fn builder(budget: Duration) -> io::Result<reqwest::ClientBuilder> {
    if budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HTTP deadline must be positive",
        ));
    }
    Ok(Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(dns::SystemDns::new(budget)?))
        .timeout(budget)
        .connect_timeout(budget)
        .read_timeout(budget)
        .redirect(reqwest::redirect::Policy::limited(10)))
}

pub async fn get(
    client: &Client,
    url: Url,
    headers: HeaderMap,
    limit: usize,
) -> io::Result<Vec<u8>> {
    if limit == 0 || !matches!(url.scheme(), "http" | "https") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HTTP input or output limit is invalid",
        ));
    }
    let mut response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(failure)?
        .error_for_status()
        .map_err(failure)?;
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP response exceeds output limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failure)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP response exceeds output limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub mod devtools;
pub mod extension_download;
pub mod proxy_check;
pub mod proxy_rotate;
pub mod public_ip;

#[cfg(test)]
mod tests;
