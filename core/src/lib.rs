/*
.slf File structure:
Signature (4 bytes = '.slf'),
version (2 bytes = 'xx' ),
padding (2 bytes),
file count (4 bytes),
padding (4 bytes),
index offset (8 bytes)
~> length of file name (4 bytes),
~> name ('length' bytes),
~> source size of file (8 bytes),
~> source checksum (4 bytes),
~> compressed size of file (8 bytes),
~> compressed checksum (4 bytes),
~> compressed file ('compressed size' bytes),
 ...
Index array (8 bytes * File count).
*/

pub mod archive;

mod error;
mod utils;

pub use archive::{ArchiveReader, ArchiveWriter, Entry};
pub use error::{Error, Result};
pub use utils::{archive_path, extraction_path};

const SIGNATURE: &[u8] = b".slf";
const VERSION: [u8; 2] = [1, 0]; // 1.0
const HEADER_SIZE: usize = 24;
const ENTRY_SIZE: usize = 28;
// const NAME_LEN_SIZE: u64 = 4;
// const SOURCE_SIZE_SIZE: u64 = 8;

const MAX_FILENAME_SIZE: u32 = 4096;
const MAX_FILE_COUNT: u32 = 1_000_000;
const MAX_FILE_SOURCE_SIZE: u64 = 128 * 1024 * 1024 * 1024;
const MAX_FILE_COMPRESSED_SIZE: u64 = 128 * 1024 * 1024 * 1024;

const BUFFER_SIZE: usize = 512 * 1024;
