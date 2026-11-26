use super::{VaultError, is_envelope};

pub const LOCKED_MESSAGE: &str = "the vault is locked, so this needs the master password - set NUMERA_PASSWORD or pass --password";

/// Recognize a supported version prefix without authenticating its token.
pub fn is_supported_envelope(value: &str) -> bool {
    value.starts_with("enc:v1:") || value.starts_with("enc:v2:")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultState {
    Disabled,
    Locked,
    Unlocked,
}

/// Authentication and encryption share the same present descriptor fields.
/// A missing field can never be substituted with a usable cryptographic value.
pub(crate) struct SecretWriter<'a> {
    state: VaultState,
    credentials: Option<(&'a str, &'a str)>,
}
impl<'a> SecretWriter<'a> {
    pub(crate) fn new(password: Option<&'a str>, descriptor: &'a serde_json::Value) -> Self {
        let credentials = match (
            password.filter(|password| !password.is_empty()),
            descriptor["salt"].as_str(),
            descriptor["verifier"].as_str(),
        ) {
            (Some(password), Some(salt), Some(verifier))
                if super::check_password(password, salt, verifier) =>
            {
                Some((password, salt))
            }
            _ => None,
        };
        let state = if descriptor.is_null() {
            VaultState::Disabled
        } else if credentials.is_some() {
            VaultState::Unlocked
        } else {
            VaultState::Locked
        };
        Self { state, credentials }
    }
    pub(crate) fn state(&self) -> VaultState {
        self.state
    }
    pub(crate) fn seal(&self, value: &str) -> Result<String, VaultError> {
        let Some((password, salt)) = self.credentials else {
            return Err(VaultError::message(LOCKED_MESSAGE.into()));
        };
        super::encrypt(password, salt, value)
    }
}

/// None is withheld, not clear. Callers supply only authenticated vault state.
pub fn write_secret(
    state: VaultState,
    value: Option<&str>,
    stored: Option<&str>,
    seal: impl FnOnce(&str) -> Result<String, VaultError>,
) -> Result<Option<String>, VaultError> {
    let Some(value) = value else {
        return Ok(stored.map(str::to_owned));
    };
    if state == VaultState::Disabled {
        return Ok(Some(value.into()));
    }
    if value.is_empty() {
        if stored.is_some_and(|value| !value.is_empty()) && state == VaultState::Locked {
            return Err(VaultError::message("the vault is locked, so this proxy password cannot be cleared: unlock first, so that what is being removed is something you can see".into()));
        }
        return Ok(Some(String::new()));
    }
    if is_envelope(value) {
        return Ok(Some(value.into()));
    }
    if state == VaultState::Locked {
        return Err(VaultError::message("the vault is locked, so this proxy password cannot be stored safely - set NUMERA_PASSWORD or pass --password first".into()));
    }
    seal(value).map(Some)
}
