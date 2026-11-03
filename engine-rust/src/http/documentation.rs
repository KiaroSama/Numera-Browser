//! Source-generated documentation contracts, served without a Python application.
use axum::{Json, Router, response::Html, routing::get};
use serde_json::Value;
use std::io;

pub fn routes() -> io::Result<Router> {
    let mut schema: Value = serde_json::from_str(include_str!("documentation-assets/openapi.json"))
        .map_err(|_| io::Error::other("Embedded HTTP documentation schema is invalid"))?;
    schema["info"]["version"] = env!("CARGO_PKG_VERSION").into();
    Ok(Router::new()
        .route(
            "/openapi.json",
            get(move || {
                let schema = schema.clone();
                async move { Json(schema) }
            }),
        )
        .route(
            "/docs",
            get(|| async { Html(include_str!("documentation-assets/swagger.html")) }),
        )
        .route(
            "/docs/oauth2-redirect",
            get(|| async { Html(include_str!("documentation-assets/oauth2-redirect.html")) }),
        )
        .route(
            "/redoc",
            get(|| async { Html(include_str!("documentation-assets/redoc.html")) }),
        ))
}
