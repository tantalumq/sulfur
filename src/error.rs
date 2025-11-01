use std::{array::TryFromSliceError, fmt, io, num::TryFromIntError, path::StripPrefixError};

use crate::{BUFFER_SIZE, VERSION};

#[derive(Debug)]
pub enum ArchiveError {
    Io(String),
    Path(String),
    IncorrectType(String),
    UnsupportedVersion(usize),
    BufferOverflow(usize),
    CorruptedArchive(String),
    EmptyFilename,
    TryFromSlice(String),
    TryFromInt(String),
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(c) => write!(f, "{}", c),
            Self::Path(c) => write!(f, "{}", c),
            Self::BufferOverflow(found) => write!(
                f,
                "Buffer overflow: {} bytes less, then {} bytes",
                BUFFER_SIZE, found
            ),
            Self::UnsupportedVersion(v) => write!(
                f,
                "Archive file has unsupported version: Current version suppots only {:?}.x archives, when {} was supplied",
                VERSION[0], v
            ),
            Self::IncorrectType(c) => write!(
                f,
                "Incorrect type of the provided archive: expected '.slf', found '.{}'",
                c
            ),
            Self::CorruptedArchive(c) => write!(f, "{}", c),
            Self::EmptyFilename => write!(f, "Filename is empty"),
            Self::TryFromSlice(c) => write!(f, "{}", c),
            Self::TryFromInt(c) => write!(f, "{}", c),
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

impl From<TryFromSliceError> for ArchiveError {
    fn from(value: TryFromSliceError) -> Self {
        Self::TryFromSlice(value.to_string())
    }
}

impl From<TryFromIntError> for ArchiveError {
    fn from(value: TryFromIntError) -> Self {
        Self::TryFromInt(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;
