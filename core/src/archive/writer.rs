use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

use fd_lock::RwLock;
use flate2::{Compression, Crc, write::GzEncoder};
use walkdir::WalkDir;

use crate::{
    ArchiveWriter, BUFFER_SIZE, Error, Result, VERSION,
    archive::{HasherWriter, entry::Entry, header::Header},
};

#[allow(clippy::missing_errors_doc)]
impl<W: Write + Seek> ArchiveWriter<W> {
    pub fn new(mut writer: W) -> Result<Self> {
        let header = Header {
            version: VERSION,
            file_count: 0,
            index_offset: 0,
        };
        header.write(&mut writer)?;

        Ok(Self {
            writer,
            header,
            entries: Vec::new(),
        })
    }

    pub fn pack(mut self, source: &Path) -> Result<W> {
        let mut file_paths = Vec::new();

        for entry in WalkDir::new(source) {
            let entry = entry?;
            if entry.file_type().is_file() {
                file_paths.push(entry.into_path());
            }
        }

        file_paths.sort();

        self.header.file_count = file_paths.len().try_into()?;
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        let mut buffer = vec![0u8; BUFFER_SIZE];

        for path in file_paths {
            let relative_name = if source.is_file() {
                path.file_name()
            } else {
                path.strip_prefix(source).ok().map(Path::as_os_str)
            }
            .and_then(|s| s.to_str())
            .map(|s| s.replace('\\', "/"))
            .ok_or(Error::Path(format!(
                "can't get relative file name from {}",
                path.display(),
            )))?;
            let metadata = path.metadata()?;
            let mtime_source = metadata.modified()?;
            let file_size = metadata.len();

            self.entries.push(Entry {
                name: relative_name,
                source_size: file_size,
                compressed_size: 0,
                source_checksum: 0,
                compressed_checksum: 0,
                offset: 0,
                data_start: 0,
            });

            let last_entry = self
                .entries
                .last_mut()
                .ok_or(Error::Empty(String::from("file entry")))?;

            last_entry.write(&mut self.writer)?;

            let mut hasher_writer = HasherWriter::new(&mut self.writer);

            let file = File::open(&path)?;
            let lock = RwLock::new(file);
            let readable = lock.read()?;
            let mut reader = BufReader::with_capacity(BUFFER_SIZE, &*readable);

            let mut source_checksum = Crc::new();

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

            last_entry.source_checksum = source_checksum.sum();
            last_entry.compressed_size = hasher_writer.take_and_reset_bytes();
            last_entry.compressed_checksum = hasher_writer.sum();

            last_entry.update(&mut self.writer)?;

            drop(readable);

            if path.metadata()?.modified()? != mtime_source {
                return Err(Error::FileModified(path.display().to_string()));
            }
        }

        let index_offset = self.writer.stream_position()?;

        for entry in &self.entries {
            let offset = entry.offset;
            self.writer.write_all(&offset.to_be_bytes())?;
        }

        self.header.index_offset = index_offset;
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        self.writer.flush()?;
        Ok(self.writer)
    }
}
