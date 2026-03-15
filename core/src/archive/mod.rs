use std::path::Path;

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

struct PermissionsGuard<'a> {
    path: &'a Path,
    source: std::fs::Permissions,
}

impl Drop for PermissionsGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(self.path, self.source.clone());
    }
}
