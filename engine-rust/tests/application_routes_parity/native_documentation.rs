use super::*;
use axum::http::StatusCode;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[path = "../../build_support/openapi.rs"]
mod openapi_build;

const PUBLIC: &[(&str, &[&str])] = &[
    ("/api/profiles/{id}/cookies/export", &["get"]),
    ("/api/profiles/{id}/page/{action}", &["post"]),
    ("/api/workflows", &["get", "post"]),
    ("/api/workflows/{id}", &["delete"]),
    ("/api/workflows/{id}/run", &["post"]),
    ("/api/fleet/sync", &["post"]),
    ("/api/profiles/{id}/ip-check", &["get"]),
    ("/api/engines/cloak/acquisition", &["get", "post"]),
    ("/api/updates/preferences", &["get", "put"]),
    ("/api/updates/check", &["get"]),
    ("/api/updates/apply", &["post"]),
    ("/api/mcp", &["post"]),
];

fn additions() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../src/http/documentation-assets/openapi-native.json"
    ))
}

pub(super) fn served_length() -> Result<String, Box<dyn std::error::Error>> {
    let mut historical: Value = serde_json::from_str(include_str!(
        "../../src/http/documentation-assets/openapi.json"
    ))?;
    historical["info"]["version"] = env!("CARGO_PKG_VERSION").into();
    let extended = openapi_build::extend(historical, &additions()?)?;
    Ok(serde_json::to_vec(&extended)?.len().to_string())
}

pub(super) fn historical_projection(
    mut actual: Value,
    historical: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let additions = additions()?;
    let paths = actual["paths"]
        .as_object_mut()
        .ok_or("Served paths missing")?;
    for (path, expected) in additions["paths"]
        .as_object()
        .ok_or("Native paths missing")?
    {
        assert_eq!(
            paths.remove(path).as_ref(),
            Some(expected),
            "Native path contract drift: {path}"
        );
    }
    let categories = actual["components"]
        .as_object_mut()
        .ok_or("Served components missing")?;
    for (category, members) in additions["components"]
        .as_object()
        .ok_or("Native components missing")?
    {
        let target = categories
            .get_mut(category)
            .and_then(Value::as_object_mut)
            .ok_or("Served native component category missing")?;
        for (name, expected) in members
            .as_object()
            .ok_or("Native component category invalid")?
        {
            assert_eq!(
                target.remove(name).as_ref(),
                Some(expected),
                "Native component contract drift: {category}/{name}"
            );
        }
        if historical["components"].get(category).is_none() {
            assert!(
                target.is_empty(),
                "Unexpected extra component category entries"
            );
            categories.remove(category);
        }
    }
    Ok(actual)
}

fn check_references(schema: &Value, value: &Value) {
    match value {
        Value::Object(fields) => {
            if let Some(reference) = fields.get("$ref").and_then(Value::as_str) {
                let pointer = reference
                    .strip_prefix('#')
                    .expect("Local OpenAPI reference");
                assert!(
                    schema.pointer(pointer).is_some(),
                    "Unresolved OpenAPI reference: {reference}"
                );
            }
            for value in fields.values() {
                check_references(schema, value);
            }
        }
        Value::Array(values) => {
            for value in values {
                check_references(schema, value);
            }
        }
        _ => {}
    }
}

#[test]
fn served_openapi_covers_native_routes_and_requires_the_reviewed_update_offer()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native-openapi-routes")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let app = tokio::time::timeout(Duration::from_secs(5), router(
            temporary.path().to_path_buf(), "synthetic-documentation-token", "", None,
            "documentation-host", Default::default(),
        )).await??;
        let response = app.clone().oneshot(Request::builder().uri("/openapi.json")
            .body(Body::empty())?).await?;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 128 * 1024).await?;
        let schema: Value = serde_json::from_slice(&bytes)?;
        let additions = additions()?;
        let expected: BTreeMap<_, BTreeSet<_>> = PUBLIC.iter().map(|(path, methods)|
            (*path, methods.iter().copied().collect())).collect();
        let actual: BTreeMap<_, BTreeSet<_>> = additions["paths"].as_object().ok_or("Native paths missing")?
            .iter().map(|(path, methods)| {
                let methods = methods.as_object().expect("Native operations object").keys()
                    .map(String::as_str).collect();
                (path.as_str(), methods)
            }).collect();
        assert_eq!(actual, expected, "The native public route catalog changed without review");
        for (path, methods) in PUBLIC {
            let concrete = path.replace("{id}", "fixture").replace("{action}", "snapshot");
            // TRACE observes the router's actual method table without invoking
            // browser, acquisition, update or workflow mutation handlers.
            let response = tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(
                Request::builder().method("TRACE").uri(concrete)
                    .header("authorization", "Bearer synthetic-documentation-token")
                    .body(Body::empty())?,
            )).await??;
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "Native route is not mounted: {path}");
            let allowed = response.headers().get("allow").and_then(|value| value.to_str().ok())
                .ok_or("Native route did not report its allowed methods")?;
            for method in *methods {
                assert!(schema["paths"][*path].get(*method).is_some(), "Missing served operation: {method} {path}");
                assert!(allowed.split(',').any(|value| value.trim().eq_ignore_ascii_case(method)),
                    "Documented operation is not mounted: {method} {path}");
            }
        }
        for action in ["prepare", "quiesce", "resume", "private-preferences"] {
            assert!(schema["paths"].get(format!("/api/updates/{action}")).is_none());
        }
        let apply = &schema["paths"]["/api/updates/apply"]["post"];
        assert_eq!(apply["requestBody"]["required"], true);
        assert_eq!(apply["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/NativeUpdateApply");
        let input = &schema["components"]["schemas"]["NativeUpdateApply"];
        assert_eq!(input["required"], json!(["offerId"]));
        assert_eq!(input["additionalProperties"], false);
        assert_eq!(input["properties"]["offerId"]["type"], "string");
        assert_eq!(input["properties"]["offerId"]["minLength"], 64);
        assert_eq!(input["properties"]["offerId"]["maxLength"], 64);
        let pattern = input["properties"]["offerId"]["pattern"].as_str().ok_or("Offer identity pattern missing")?;
        let pattern = regex::Regex::new(pattern)?;
        assert!(pattern.is_match(&"a".repeat(64)));
        for invalid in ["".into(), "a".repeat(63), "a".repeat(65), "A".repeat(64), "g".repeat(64)] {
            assert!(!pattern.is_match(&invalid), "Invalid reviewed offer is advertised as valid");
        }
        assert!(apply["responses"].get("409").is_some());
        assert_eq!(schema["paths"]["/api/mcp"]["post"]["security"], json!([{"NativeApiBearer":[]}]));
        assert_eq!(schema["components"]["schemas"]["NativeCloakApproval"]["required"], json!(["offer_id", "approve"]));
        assert_eq!(schema["components"]["schemas"]["NativeCloakApproval"]["properties"]["approve"]["const"], true);
        let mut operation_ids = BTreeSet::new();
        for route in schema["paths"].as_object().ok_or("Served paths missing")?.values() {
            for operation in route.as_object().ok_or("Served route missing operations")?.values() {
                if let Some(id) = operation["operationId"].as_str() {
                    assert!(operation_ids.insert(id), "Duplicate OpenAPI operation identity");
                }
            }
        }
        check_references(&schema, &schema);
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_openapi_merge_refuses_historical_route_and_component_replacement() {
    let historical = json!({"paths":{"/api/profiles/{pid}":{"get":{}}},
        "components":{"schemas":{"Historical":{"type":"string"}}}});
    for additions in [
        json!({"paths":{"/api/profiles/{pid}":{"post":{}}},"components":{}}),
        json!({"paths":{"/api/profiles/{id}":{"post":{}}},"components":{}}),
        json!({"paths":{},"components":{"schemas":{"Historical":{"type":"object"}}}}),
        json!({"paths":{},"components":{},"info":{"version":"changed"}}),
    ] {
        assert!(openapi_build::extend(historical.clone(), &additions).is_err());
    }
    let additions = json!({"paths":{"/api/new":{"get":{}}},
        "components":{"schemas":{"Native":{"type":"boolean"}}}});
    let extended =
        openapi_build::extend(historical.clone(), &additions).expect("Additive contract");
    assert_eq!(
        extended["paths"]["/api/profiles/{pid}"],
        historical["paths"]["/api/profiles/{pid}"]
    );
    assert_eq!(
        extended["components"]["schemas"]["Historical"],
        historical["components"]["schemas"]["Historical"]
    );
    assert_eq!(
        extended["paths"]["/api/new"],
        additions["paths"]["/api/new"]
    );
}
