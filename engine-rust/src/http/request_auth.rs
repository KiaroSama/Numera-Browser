use super::tokens::TokenStore;
use serde_json::{Value, json};
use std::io;
use subtle::ConstantTimeEq;

pub struct RequestAuth {
    tokens: TokenStore,
    environment: String,
}
impl RequestAuth {
    /// The configured environment token is captured once; named-token protection
    /// is read per request, so creating or losing credentials cannot reopen it.
    pub fn new(tokens: TokenStore, environment: &str) -> Self {
        Self {
            tokens,
            environment: environment.trim().into(),
        }
    }
    pub fn adopt(&self, now: f64) -> io::Result<bool> {
        self.tokens.adopt(now)
    }
    pub fn required(&self) -> bool {
        !self.environment.is_empty() || self.tokens.protected()
    }
    pub fn denial_body() -> Value {
        json!({"detail":"invalid or missing API token"})
    }
    pub fn authorize(
        &self,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        alternate: Option<&str>,
        now: f64,
    ) -> io::Result<bool> {
        if method == "OPTIONS"
            || !path.starts_with("/api/")
            || path.trim_end_matches('/') == "/api/ready"
            || !self.required()
        {
            return Ok(true);
        }
        let authorization = authorization.unwrap_or("");
        let sent = if authorization
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        {
            &authorization[7..]
        } else {
            alternate.unwrap_or("")
        }
        .trim();
        if !self.environment.is_empty()
            && sent
                .as_bytes()
                .ct_eq(self.environment.as_bytes())
                .unwrap_u8()
                == 1
        {
            return Ok(true);
        }
        Ok(self.tokens.verify(sent, now)?.is_some())
    }
}
