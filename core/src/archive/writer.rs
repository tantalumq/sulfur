use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    result,
};

use flate2::{Compression, Crc, write::GzEncoder};
use walkdir::WalkDir;

use crate::{
    ArchiveWriter, BUFFER_SIZE, Error, Result, VERSION,
    archive::{entry::Entry, header::Header},
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

    pub fn pack(&mut self, source: &Path) -> Result<()> {
        let file_paths = if source.is_file() {
            vec![source.to_path_buf()]
        } else {
            WalkDir::new(source)
                .into_iter()
                .filter_map(result::Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(walkdir::DirEntry::into_path)
                .collect()
        };

        self.header.file_count = Some(file_paths.len().try_into()?);
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

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
            let file_size = path.metadata()?.len();

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
                .ok_or(Error::Empty(String::from("Can't get file entry")))?;

            last_entry.write(&mut self.writer)?;

            let mut hasher_writer = HasherWriter::new(&mut self.writer);

            let file = File::open(path)?;
            let mut reader = BufReader::new(file);

            let mut buffer = vec![0u8; BUFFER_SIZE];

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
        }

        let index_offset = self.writer.stream_position()?;

        for entry in &self.entries {
            let offset = entry.offset.ok_or(Error::Empty(String::from(
                "Can't get offset from file entry",
            )))?;
            self.writer.write_all(&offset.to_be_bytes())?;
        }

        self.header.index_offset = Some(index_offset);
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        self.writer.flush()?;
        Ok(())
    }
}

pub struct HasherWriter<W> {
    writer: W,
    hasher: Crc,
    bytes: u64,
}

#[allow(clippy::missing_errors_doc)]
impl<W: Write + Seek> HasherWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: Crc::new(),
            bytes: 0,
        }
    }

    pub fn sum(&self) -> u32 {
        self.hasher.sum()
    }

    pub fn stream_position(&mut self) -> Result<u64> {
        let pos = self.writer.stream_position()?;
        Ok(pos)
    }

    pub fn take_and_reset_bytes(&mut self) -> u64 {
        let old = self.bytes;
        self.bytes = 0;
        old
    }
}

impl<W: Write + Seek> Write for HasherWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let bytes = self.writer.write(buf)?;
        self.hasher.update(&buf[..bytes]);
        self.bytes += bytes as u64;
        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
