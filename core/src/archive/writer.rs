use std::{
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

use flate2::{Compression, Crc, write::GzEncoder};
use walkdir::WalkDir;

use crate::{
    ArchiveWriter, BUFFER_SIZE, Error, Result, VERSION,
    archive::{HasherWriter, PermissionsGuard, entry::Entry, header::Header},
};

#[allow(clippy::missing_errors_doc)]
impl<W: Write + Seek> ArchiveWriter<W> {
    pub fn new(mut writer: W) -> Result<Self> {
        let header = Header {
            version: VERSION,
            file_count: None,
            index_offset: None,
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

        self.header.file_count = Some(file_paths.len().try_into()?);
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
            let source_permissions = metadata.permissions();

            let guard = PermissionsGuard {
                path: &path,
                source: source_permissions.clone(),
            };

            let mut lock_permissions = source_permissions.clone();
            lock_permissions.set_readonly(true);
            fs::set_permissions(&path, lock_permissions)?;

            let file_size = metadata.len();

            self.entries.push(Entry {
                name: relative_name,
                source_size: Some(file_size),
                compressed_size: None,
                source_checksum: None,
                compressed_checksum: None,
                offset: None,
                data_start: None,
            });

            let last_entry = self
                .entries
                .last_mut()
                .ok_or(Error::Empty(String::from("file entry")))?;

            last_entry.write(&mut self.writer)?;

            let mut hasher_writer = HasherWriter::new(&mut self.writer);

            let file = File::open(&path)?;
            let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);

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

            last_entry.source_checksum = Some(source_checksum.sum());
            last_entry.compressed_size = Some(hasher_writer.take_and_reset_bytes());
            last_entry.compressed_checksum = Some(hasher_writer.sum());

            last_entry.update(&mut self.writer)?;

            drop(guard);
        }

        let index_offset = self.writer.stream_position()?;

        for entry in &self.entries {
            let offset = entry
                .offset
                .ok_or(Error::Empty(String::from("get offset from file entry")))?;
            self.writer.write_all(&offset.to_be_bytes())?;
        }

        self.header.index_offset = Some(index_offset);
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        self.writer.flush()?;
        Ok(self.writer)
    }
}
