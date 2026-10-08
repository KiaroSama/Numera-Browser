use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum TokenError {
    DuplicateName(String),
    Lockout,
    Storage(io::Error),
}
impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(formatter, "a token called {} already exists", crate::validation::repr(&serde_json::Value::String(name.clone()))),
            Self::Lockout => formatter.write_str("this is the last API token, and removing it would lock this server to nobody. Create another token first, or set NUMERA_API_TOKEN, or turn protection off deliberately."),
            Self::Storage(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl Error for TokenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for TokenError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}
