use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};

pub struct ObjectBody(pub Map<String, Value>);
impl<S: Send + Sync> FromRequest<S> for ObjectBody {
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        extract_object(request, state, false)
            .await
            .map(Self)
            .map_err(|response| *response)
    }
}

pub struct OptionalObjectBody(pub Map<String, Value>);
impl<S: Send + Sync> FromRequest<S> for OptionalObjectBody {
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        extract_object(request, state, true)
            .await
            .map(Self)
            .map_err(|response| *response)
    }
}
async fn extract_object<S: Send + Sync>(
    request: Request,
    state: &S,
    optional: bool,
) -> Result<Map<String, Value>, Box<Response>> {
    let json_media = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .and_then(|value| value.trim().split_once('/'))
        .is_some_and(|(kind, subtype)| {
            kind.eq_ignore_ascii_case("application")
                && (subtype.eq_ignore_ascii_case("json")
                    || subtype.to_ascii_lowercase().ends_with("+json"))
        });
    let bytes = Bytes::from_request(request, state)
        .await
        .map_err(IntoResponse::into_response)?;
    let value = if bytes.is_empty() {
        Value::Null
    } else if json_media {
        let text = super::json_encoding::decode(&bytes).map_err(|_| parsing_failure())?;
        serde_json::from_str(&text).map_err(|_| invalid_json(&text))?
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| parsing_failure())?;
        return Err(Box::new(invalid_dictionary(Value::String(text.to_owned()))));
    };
    match value {
            Value::Object(object) => Ok(object),
            Value::Null if optional => Ok(Map::new()),
            Value::Null => Err(Box::new((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"detail":[{"type":"missing","loc":["body"],"msg":"Field required","input":null}]}))).into_response())),
            value => Err(Box::new(invalid_dictionary(value))),
        }
}
fn invalid_json(text: &str) -> Response {
    match super::json_diagnostics::diagnose(text) {
        Some(error) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"detail":[{"type":"json_invalid","loc":["body",error.position],"msg":"JSON decode error","input":{},"ctx":{"error":error.message}}]}))).into_response(),
        None => parsing_failure(),
    }
}
fn parsing_failure() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"detail":"There was an error parsing the body"})),
    )
        .into_response()
}
fn invalid_dictionary(value: Value) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"detail":[{"type":"dict_type","loc":["body"],"msg":"Input should be a valid dictionary","input":value}]}))).into_response()
}
