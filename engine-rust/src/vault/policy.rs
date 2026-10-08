use super::{VaultError, is_envelope};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultState {
    Disabled,
    Locked,
    Unlocked,
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
            return Err(VaultError("the vault is locked, so this proxy password cannot be cleared: unlock first, so that what is being removed is something you can see".into()));
        }
        return Ok(Some(String::new()));
    }
    if is_envelope(value) {
        return Ok(Some(value.into()));
    }
    if state == VaultState::Locked {
        return Err(VaultError("the vault is locked, so this proxy password cannot be stored safely - set NUMERA_PASSWORD or pass --password first".into()));
    }
    seal(value).map(Some)
}
