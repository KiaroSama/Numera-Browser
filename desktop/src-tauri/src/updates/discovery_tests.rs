use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

fn metadata(source: &str, generation: u64) -> serde_json::Value {
    json!({
        "schema": 1, "repository": manifest::REPOSITORY, "product": manifest::PRODUCT,
        "version": "0.36.0", "source_sha": source, "generation": generation,
        "created_at": 1000, "expires_at": 2000, "minimum_protocol": 1,
        "components": {"linux-x64": {"sdk": {}, "browsers": {}}},
        "assets": [{"platform": "linux-x64", "name": "numera-fixture.tar.gz",
            "size": 10, "sha256": "b".repeat(64), "expanded_size": 20,
            "max_members": 4, "archive_root": manifest::PRODUCT,
            "build_manifest_sha256": "c".repeat(64)}]
    })
}

#[test]
fn discovery_distinguishes_unavailable_untrusted_and_authenticated_unsuitable_candidates() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let source = "a".repeat(40);
    for (case, expected) in [
        ("available", "offer"),
        ("empty", "none"),
        ("unrelated", "none"),
        ("draft", "none"),
        ("current", "none"),
        ("expired", "none"),
        ("future", "none"),
        ("protocol", "none"),
        ("platform", "none"),
        ("metadata-offline", "error"),
        ("signature-offline", "error"),
        ("wrong-key", "error"),
        ("tampered", "error"),
        ("malformed", "error"),
        ("oversized", "error"),
        ("bad-asset", "error"),
        ("source", "error"),
        ("cancel-signature", "cancelled"),
    ] {
        let cancelled = AtomicBool::new(false);
        let mut value = metadata(&source, 2);
        match case {
            "current" => value["generation"] = 1.into(),
            "expired" => value["expires_at"] = 1100.into(),
            "future" => value["created_at"] = 1600.into(),
            "protocol" => value["minimum_protocol"] = 2.into(),
            "platform" => {
                value["assets"][0]["platform"] = "windows-x64".into();
                value["components"] = json!({"windows-x64": {"sdk": {}, "browsers": {}}});
            }
            "bad-asset" => value["assets"][0]["size"] = 0.into(),
            "source" => value["source_sha"] = "d".repeat(40).into(),
            _ => {}
        }
        let mut bytes = serde_json::to_vec(&value).expect("Fixture metadata");
        if case == "malformed" {
            bytes = b"private-fixture-response-not-json".to_vec();
        }
        if case == "oversized" {
            bytes.resize(manifest::METADATA_LIMIT as usize + 1, b' ');
        }
        let signer = if case == "wrong-key" {
            SigningKey::from_bytes(&[8; 32])
        } else {
            key.clone()
        };
        let signature = signer.sign(&bytes).to_bytes().to_vec();
        if case == "tampered" {
            bytes.push(b' ');
        }
        let releases = match case {
            "empty" => json!([]),
            "unrelated" => json!([{"draft": false, "tag_name": "v0.36.0"}]),
            "draft" => json!([{"draft": true, "tag_name": format!("build-{source}")}]),
            _ => json!([{"draft": false, "tag_name": format!("build-{source}")}]),
        };
        let (metadata_url, signature_url) = manifest_urls(&source);
        let mut requests = Vec::new();
        let result = discover_with(
            1,
            "linux-x64",
            &key.verifying_key().to_bytes(),
            1200,
            &cancelled,
            |url, limit, budget| {
                assert!(!budget.is_zero() && budget <= Duration::from_secs(60));
                requests.push(url.to_owned());
                if url == discovery_url(1)? {
                    assert_eq!(limit, manifest::METADATA_LIMIT);
                    return Ok(serde_json::to_vec(&releases).expect("Fixture release list"));
                }
                if url == metadata_url {
                    assert_eq!(limit, manifest::METADATA_LIMIT);
                    if case == "metadata-offline" {
                        return Err(io::Error::other("private transport detail"));
                    }
                    return Ok(bytes.clone());
                }
                assert_eq!(
                    url, signature_url,
                    "Discovery downloaded an unapproved asset"
                );
                assert_eq!(limit, 64);
                if case == "signature-offline" {
                    return Err(io::Error::other("private transport detail"));
                }
                if case == "cancel-signature" {
                    cancelled.store(true, Ordering::Release);
                }
                Ok(signature.clone())
            },
        );
        match expected {
            "offer" => assert_eq!(result.expect(case).expect(case).manifest.generation, 2),
            "none" => assert!(result.expect(case).is_none(), "{case}"),
            _ => {
                let error = result.err().expect(case);
                if expected == "cancelled" {
                    assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
                }
                assert!(
                    !error.to_string().contains("private"),
                    "Raw diagnostic leaked: {case}"
                );
            }
        }
        if matches!(case, "empty" | "unrelated" | "draft") {
            assert_eq!(requests.len(), 1);
        }
    }
}

#[test]
fn incomplete_discovery_never_claims_the_highest_offer_or_current_generation() {
    let key = SigningKey::from_bytes(&[7; 32]);
    for case in ["highest", "mixed-failure", "page-limit", "cancelled"] {
        let cancelled = AtomicBool::new(case == "cancelled");
        let mut calls = 0;
        let mut pages = 0;
        let result = discover_with(
            1,
            "linux-x64",
            &key.verifying_key().to_bytes(),
            1200,
            &cancelled,
            |url, _, _| {
                calls += 1;
                for page in 1..=3 {
                    if url == discovery_url(page)? {
                        pages += 1;
                        let rows = if case == "page-limit" {
                            vec![json!({"tag_name": "unrelated"}); 20]
                        } else {
                            vec![
                                json!({"tag_name": format!("build-{:040x}", 3)}),
                                json!({"tag_name": format!("build-{:040x}", 2)}),
                            ]
                        };
                        return Ok(serde_json::to_vec(&rows).expect("Fixture page"));
                    }
                }
                for generation in [2, 3] {
                    let source = format!("{generation:040x}");
                    let (metadata_url, signature_url) = manifest_urls(&source);
                    let bytes = serde_json::to_vec(&metadata(&source, generation))
                        .expect("Fixture metadata");
                    if url == metadata_url {
                        if case == "mixed-failure" && generation == 3 {
                            return Err(io::Error::other("offline"));
                        }
                        return Ok(bytes);
                    }
                    if url == signature_url {
                        return Ok(key.sign(&bytes).to_bytes().to_vec());
                    }
                }
                panic!("Unexpected discovery request");
            },
        );
        if case == "highest" {
            assert_eq!(result.expect(case).expect(case).manifest.generation, 3);
        } else {
            assert!(
                result.is_err(),
                "Incomplete discovery reported success: {case}"
            );
        }
        if case == "cancelled" {
            assert_eq!(calls, 0);
        }
        if case == "page-limit" {
            assert_eq!(pages, 3);
        }
    }
}
