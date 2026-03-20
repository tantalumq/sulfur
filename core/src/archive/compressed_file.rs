use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

use fd_lock::RwLock;
use flate2::{Compression, Crc, write::GzEncoder};
use tempfile::NamedTempFile;

use crate::{
    BUFFER_SIZE, Error, Result,
    archive::{CompressedFile, HasherWriter, entry::Entry},
};
impl CompressedFile {
    pub fn create(path: &Path, relative_name: String) -> Result<Self> {
        let metadata = path.metadata()?;
        let mtime_source = metadata.modified()?;
        let file_size = metadata.len();

        let temp = NamedTempFile::new()?;
        let mut hasher_writer = HasherWriter::new(&temp);

        let file = File::open(path)?;
        let lock = RwLock::new(file);
        let readable = lock.read()?;
        let mut reader = BufReader::with_capacity(BUFFER_SIZE, &*readable);

        let mut source_checksum = Crc::new();

        let mut buffer = vec![0u8; BUFFER_SIZE];

        let mut encoder = GzEncoder::new(hasher_writer, Compression::default());

        loop {
            let bytes = reader.read(&mut buffer)?;

            if bytes == 0 {
                break; // EOF
            }

            let chunk = &buffer[..bytes];

            source_checksum.update(chunk);
            encoder.write_all(chunk)?;
        }

        hasher_writer = encoder.finish()?;
        hasher_writer.flush()?;

        drop(readable);

        if path.metadata()?.modified()? != mtime_source {
            return Err(Error::FileModified(path.display().to_string()));
        }

        let entry = Entry {
            name: relative_name,
            source_size: file_size,
            compressed_size: hasher_writer.take_and_reset_bytes(),
            source_checksum: source_checksum.sum(),
            compressed_checksum: hasher_writer.sum(),
            offset: 0,
            data_start: 0,
        };

        Ok(Self {
            entry,
            content: temp,
        })
    }
}
