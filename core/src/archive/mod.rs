use crc32fast::Hasher;
use tempfile::TempPath;

pub use crate::archive::{entry::Entry, header::Header};

pub mod entry;
pub mod header;
pub mod reader;
pub mod writer;

pub(crate) mod compressed_file;
pub(crate) mod entry_reader;
pub(crate) mod hasher_writer;

pub struct CompressedFile {
    id: u32,
    entry: Entry,
    content_path: TempPath,
}

pub struct ArchiveWriter<W> {
    writer: W,
    header: Header,
    entries: Vec<Entry>,
    compression_level: i32,
}

pub struct ArchiveReader<R> {
    reader: R,
    header: Header,
    entries: Vec<Entry>,
}

struct HasherWriter<W> {
    writer: W,
    hasher: Hasher,
    bytes: u64,
}
