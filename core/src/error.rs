use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    Path(String),
    #[error("invalid file signature: {0}")]
    IncorrectSignature(String),
    #[error("unsupported version: expected {expected}.x, found {found}")]
    UnsupportedVersion { expected: u8, found: u8 },
    #[error("buffer overflow: {0} is greater than current BufferSize")]
    BufferOverflow(usize),
    #[error("empty filename")]
    EmptyFilename,
    #[error("checksum mismatch: expected {expected}, found {found}")]
    ChecksumMismatch { expected: u32, found: u32 },
    #[error("size mismatch: expected {expected}, found {found}")]
    SizeMismatch { expected: u64, found: u64 },
    #[error(transparent)]
    TryFromSlice(#[from] std::array::TryFromSliceError),
    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error(
        "invalid file index {index}. the archive contains {file_count} files (valid indices are 0 to {}).", file_count - 1
    )]
    IndexOutOfRange { index: u32, file_count: u32 },
    #[error("invalid utf-8 in filename: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(
        "too many files in archive (in header): limit is set at {expected}, but found {found} "
    )]
    TooManyFiles { expected: u32, found: u32 },
    #[error("Can't get {0}")]
    Empty(String),
}

pub type Result<T> = std::result::Result<T, Error>;
