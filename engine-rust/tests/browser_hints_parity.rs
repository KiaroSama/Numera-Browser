use numera_core::{browser::hints, models::Fingerprint};
use serde_json::Value;
#[path = "common/hint_site.rs"]
mod hint_site;

#[test]
fn hint_response_peer_disconnects_preserve_other_io_failures() -> std::io::Result<()> {
    use std::io::{self, Write};

    struct Fault {
        kind: io::ErrorKind,
        at_flush: bool,
        written: usize,
    }
    impl Write for Fault {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.at_flush {
                return Err(self.kind.into());
            }
            self.written += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(self.kind.into())
        }
    }
    for at_flush in [false, true] {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::TimedOut,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::WriteZero,
            io::ErrorKind::Other,
        ] {
            let mut writer = Fault {
                kind,
                at_flush,
                written: 0,
            };
            let result = hint_site::respond(&mut writer, "/fixture/first");
            if matches!(
                kind,
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
            ) {
                assert!(
                    result.is_ok(),
                    "Hint response peer disconnect escaped: {kind:?}/{at_flush}"
                );
            } else {
                assert_eq!(
                    result
                        .expect_err("Fixture swallowed an unrelated I/O failure")
                        .kind(),
                    kind
                );
            }
            assert_eq!(writer.written > 0, at_flush);
        }
    }
    let mut site = hint_site::HintSite::new()?;
    let url = site.url("/fixture/first")?;
    let address = url
        .strip_prefix("http://")
        .and_then(|s| s.split('/').next())
        .ok_or_else(|| io::Error::other("Fixture address missing"))?;
    let mut client = std::net::TcpStream::connect_timeout(
        &address.parse().map_err(io::Error::other)?,
        std::time::Duration::from_secs(2),
    )?;
    client.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    client.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
    client.write_all(b"GET /fixture/first HTTP/1.1\r\nHost: fixture\r\nSec-CH-UA-Platform: fixture-platform\r\n\r\n")?;
    let mut actual = Vec::new();
    std::io::Read::read_to_end(&mut client, &mut actual)?;
    let actual = String::from_utf8(actual).map_err(io::Error::other)?;
    let (headers, body) = actual
        .split_once("\r\n\r\n")
        .ok_or_else(|| io::Error::other("Hint response header boundary missing"))?;
    assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(headers.contains("Content-Type: text/html; charset=utf-8\r\n"));
    assert!(headers.contains(&format!("Content-Length: {}\r\n", body.len())));
    assert!(headers.contains("Accept-CH: Sec-CH-UA-Platform-Version, Sec-CH-UA-Arch, Sec-CH-UA-Bitness, Sec-CH-UA-Model, Sec-CH-UA-Full-Version, Sec-CH-UA-Full-Version-List, Sec-CH-UA-WoW64\r\n"));
    assert!(headers.ends_with("Connection: close"));
    assert_eq!(
        body,
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>hint fixture</title><script src=\"/fixture/resource\"></script>"
    );
    assert_eq!(
        site.headers("/fixture/first")?
            .get("sec-ch-ua-platform")
            .map(String::as_str),
        Some("fixture-platform")
    );
    assert!(site.headers("/never-requested").is_err());
    drop(client);
    site.finish()?;
    assert!(
        std::net::TcpStream::connect_timeout(
            &address.parse().map_err(io::Error::other)?,
            std::time::Duration::from_millis(100),
        )
        .is_err(),
        "Hint fixture listener survived cleanup"
    );
    Ok(())
}

#[test]
fn cdp_hint_commands_and_new_page_policy_match_original() -> Result<(), Box<dyn std::error::Error>>
{
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-hints.json"))?;
    let cases = document["cases"].as_array().ok_or("Hint cases missing")?;
    assert_eq!(cases.len(), 10);
    for case in cases {
        let fp = Fingerprint::from_record(&case["input"])?;
        let policy = hints::policy(&fp)?;
        assert_eq!(
            policy["commands"], case["initial"],
            "Initial hints {}",
            case["name"]
        );
        assert_eq!(
            policy["commands"], case["later"],
            "New page hints {}",
            case["name"]
        );
        assert_eq!(
            policy["strip_headers_prefixes"],
            if fp.os == "ios" {
                serde_json::json!(["sec-ch-ua"])
            } else {
                serde_json::json!([])
            }
        );
    }
    Ok(())
}
