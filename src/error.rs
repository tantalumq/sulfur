use std::{fmt, io, path::StripPrefixError};

#[derive(Debug)]
pub enum ArchiveError {
    Io(String),
    Path(String),
    Cast(String),
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(c) => write!(f, "{}", c),
            Self::Path(c) => write!(f, "{}", c),
            Self::Cast(c) => write!(f, "{}", c),
        }
    }
}
impl From<io::Error> for ArchiveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
impl From<StripPrefixError> for ArchiveError {
    fn from(value: StripPrefixError) -> Self {
        Self::Path(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;
