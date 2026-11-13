//! Individually reported ports of the browser source suite.
//! The complete source IDs and adaptations are recorded in docs/source-browser-test-map.json.
use numera_core as core;
use numera_core::{browser, conversion, models::Profile};
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod browser_fixture;
use serde_json::{Value, json};
mod common;
macro_rules! source_test {($name:ident,$source:literal,$body:block)=>{#[test] #[doc=$source] fn $name() $body};}

fn profile() -> Profile {
    conversion::to_engine(
        &json!({"id":"browser-source", "name":"Browser source", "os":"windows", "seed":42}),
        1000.0,
    )
    .expect("valid source profile")
}
fn launch_options(p: &Profile) -> std::io::Result<Value> {
    browser::options::build(p, "playwright", "/owned/profile", false, &[], &[], None)
}
#[path = "source_browser/launch_extras.rs"]
mod launch_extras;
fn fresh() -> common::TempDirectory {
    common::TempDirectory::new("source-browser").expect("browser contract")
}
fn fp(os: &str, seed: i64) -> numera_core::models::Fingerprint {
    numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
        seed,
        os: Some(os),
        ..Default::default()
    })
    .expect("browser contract")
}
#[path = "source_browser/adapter_acquisition.rs"]
mod adapter_acquisition;
#[path = "source_browser/bundled_engines.rs"]
mod bundled_engines;
#[path = "source_browser/camoufox_startup_cleanup.rs"]
mod camoufox_startup_cleanup;
#[path = "source_browser/cookies.rs"]
mod cookies;
#[path = "source_browser/drivers.rs"]
mod drivers;
#[path = "source_browser/fingerprint_parity.rs"]
mod fingerprint_parity;
#[path = "source_browser/http.rs"]
mod http;
#[path = "source_browser/probe.rs"]
mod probe;
#[path = "source_browser/startup_resources.rs"]
mod startup_resources;
#[path = "source_browser/startup_urls.rs"]
mod startup_urls;
#[path = "source_browser/startup_urls_stay_whole.rs"]
mod startup_urls_stay_whole;
#[path = "source_browser/support.rs"]
mod support;
#[path = "source_browser/timezone_aliases.rs"]
mod timezone_aliases;
#[path = "source_browser/ua_metadata.rs"]
mod ua_metadata;
#[path = "source_browser/warmup.rs"]
mod warmup;
#[path = "source_browser/warning_gate.rs"]
mod warning_gate;
