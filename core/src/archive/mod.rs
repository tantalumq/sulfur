use flate2::Crc;

use crate::archive::{entry::Entry, header::Header};

pub mod entry;
pub mod header;
pub mod reader;
pub mod writer;

pub(crate) mod hasher_writer;

pub struct ArchiveWriter<W> {
    writer: W,
    header: Header,
    entries: Vec<Entry>,
}

pub struct ArchiveReader<R> {
    reader: R,
    header: Header,
    entries: Vec<Entry>,
}

struct HasherWriter<W> {
    writer: W,
    hasher: Crc,
    bytes: u64,
}
