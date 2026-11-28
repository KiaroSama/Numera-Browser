//! Historical contracts plus additive native routes, served without Python.
use axum::{Router, http::header, response::Html, routing::get};

pub fn routes() -> Router {
    Router::new()
        .route(
            "/openapi.json",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    include_str!(concat!(env!("OUT_DIR"), "/openapi.json")),
                )
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
        )
}
