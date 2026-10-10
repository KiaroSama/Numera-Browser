use super::adapter::Adapter;
use serde_json::Map;
use std::{io, time::Duration};

/// Unreachable startup URLs do not abort the original session. Protocol/owner
/// failures are different: an unverifiable transport must not be kept running.
pub fn navigate(adapter: &mut Adapter, urls: &[String]) -> io::Result<()> {
    for (index, url) in urls.iter().enumerate() {
        let value = adapter.call(
            "navigate",
            Map::from_iter([
                ("url".into(), url.clone().into()),
                ("new_page".into(), (index > 0).into()),
                ("tolerate_failure".into(), true.into()),
            ]),
            Duration::from_secs(35),
        )?;
        if !value["navigated"].is_boolean() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Startup navigation result missing",
            ));
        }
    }
    Ok(())
}
