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

use std::{
    fmt::Display,
    fs::{File, create_dir_all},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    result,
};

use flate2::{
    Compression, Crc,
    write::{GzDecoder, GzEncoder},
};
use walkdir::WalkDir;

use crate::error::{Error, Result};

pub mod error;

const HEADER_SIZE: usize = 24;
const ENTRY_SIZE: usize = 28;
const SIGNATURE: &[u8] = b".slf";
const VERSION: [u8; 2] = [1, 0]; // 1.0

const BUFFER_SIZE: usize = 128 * 1024;
const MAX_FILENAME_SIZE: usize = 4096;
const MAX_FILE_COUNT: u32 = 1_000_000;

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

    pub fn stream_position(&mut self) -> error::Result<u64> {
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

pub struct ArchiveWriter<W> {
    writer: W,
    header: Header,
    entries: Vec<Entry>,
}
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

pub struct ArchiveReader<R> {
    reader: R,
    header: Header,
    entries: Vec<Entry>,
}
#[allow(clippy::missing_errors_doc)]
impl<R: Read + Seek> ArchiveReader<R> {
    pub fn open(mut reader: R) -> Result<Self> {
        let header = Header::decode(&mut reader)?;

        reader.seek(SeekFrom::Start(header.index_offset.ok_or(Error::Empty(
            String::from("Can't get index offset from header"),
        ))?))?;
        let mut entry_offsets: Vec<u64> = Vec::with_capacity(
            header
                .file_count
                .ok_or(Error::Empty(String::from(
                    "Can't get file count from header",
                )))?
                .try_into()?,
        );
        for _ in 0..header.file_count.ok_or(Error::Empty(String::from(
            "Can't get file count from header",
        )))? {
            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            entry_offsets.push(u64::from_be_bytes(offset_bytes));
        }

        let mut entries: Vec<Entry> = Vec::with_capacity(
            header
                .file_count
                .ok_or(Error::Empty(String::from(
                    "Can't get file count from header",
                )))?
                .try_into()?,
        );
        for offset in entry_offsets {
            reader.seek(SeekFrom::Start(offset))?;
            let mut entry = Entry::decode(&mut reader)?;
            entry.offset = Some(offset);
            entries.push(entry);
        }

        Ok(Self {
            reader,
            header,
            entries,
        })
    }

    pub fn extract(&mut self, index: u32, target: &Path) -> Result<()> {
        let entry = self
            .entries
            .get(index as usize)
            .ok_or(Error::IndexOutOfRange {
                index,
                file_count: self.entries.len().try_into()?,
            })?;

        let path = safe_join(target, Path::new(&entry.name))?;

        if let Some(parents) = path.parent() {
            create_dir_all(parents)?;
        }

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        let mut hasher_writer = HasherWriter::new(&mut writer);

        let mut compressed_checksum = Crc::new();

        let mut decoder = GzDecoder::new(&mut hasher_writer);

        let mut remaining_bytes = entry.compressed_size.ok_or(Error::Empty(String::from(
            "Can't get compressed size from file entry",
        )))?;

        let mut buffer = vec![0u8; BUFFER_SIZE];

        self.reader
            .seek(SeekFrom::Start(entry.data_start.ok_or(Error::Empty(
                String::from("Can't get start of data from entry"),
            ))?))?;

        loop {
            let to_read = usize::try_from(remaining_bytes.min(buffer.len() as u64))?;

            let bytes = self.reader.read(&mut buffer[..to_read])?;

            if bytes == 0 || remaining_bytes == 0 {
                break;
            }

            let chunk = &buffer[..bytes];

            compressed_checksum.update(chunk);

            decoder.write_all(chunk)?;

            remaining_bytes -= bytes as u64;
        }

        if remaining_bytes != 0 {
            return Err(Error::SizeMismatch {
                expected: entry.compressed_size.ok_or(Error::Empty(String::from(
                    "Can't get compressed size from file entry",
                )))?,
                found: entry.compressed_size.ok_or(Error::Empty(String::from(
                    "Can't get compressed size from file entry",
                )))? - remaining_bytes,
            });
        }

        decoder.finish()?;

        let source_checksum = hasher_writer.sum();
        let compressed_checksum = compressed_checksum.sum();

        if source_checksum
            != entry.source_checksum.ok_or(Error::Empty(String::from(
                "Can't get source checksum from file entry",
            )))?
        {
            return Err(Error::ChecksumMismatch {
                expected: entry.source_checksum.ok_or(Error::Empty(String::from(
                    "Can't get source checksum from file entry",
                )))?,
                found: source_checksum,
            });
        }

        if compressed_checksum
            != entry.compressed_checksum.ok_or(Error::Empty(String::from(
                "Can't get compressed checksum from file entry",
            )))?
        {
            return Err(Error::ChecksumMismatch {
                expected: entry.compressed_checksum.ok_or(Error::Empty(String::from(
                    "Can't get compressed checksum from file entry",
                )))?,
                found: compressed_checksum,
            });
        }

        let size = hasher_writer.take_and_reset_bytes();
        if entry.source_size.ok_or(Error::Empty(String::from(
            "Can't get source size from file entry",
        )))? != size
        {
            return Err(Error::SizeMismatch {
                expected: entry.source_size.ok_or(Error::Empty(String::from(
                    "Can't get source size from file entry",
                )))?,
                found: size,
            });
        }

        writer.flush()?;
        Ok(())
    }

    pub fn extract_all(&mut self, target: &Path) -> Result<()> {
        for i in 0..self.header.file_count.ok_or(Error::Empty(String::from(
            "Can't get file count from header",
        )))? {
            self.extract(i, target)?;
        }
        Ok(())
    }
}

impl<W> Display for ArchiveReader<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let index_width = self.entries.len().to_string().len().max("INDEX".len());
        let name_width = self.entries.iter().map(|f| f.name.len()).max().unwrap_or(0);
        let original_size_width = self
            .entries
            .iter()
            .map(|f| {
                f.source_size
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string())
                    .len()
            })
            .max()
            .unwrap_or(0)
            .max("ORIGINAL SIZE".len());
        let compressed_size_width = self
            .entries
            .iter()
            .map(|f| {
                f.compressed_size
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string())
                    .len()
            })
            .max()
            .unwrap_or(0)
            .max("COMPRESSED SIZE".len());

        let original_checksum_width = self
            .entries
            .iter()
            .map(|f| {
                f.source_checksum
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string())
                    .len()
            })
            .max()
            .unwrap_or(0)
            .max("ORIGINAL CHECKSUM".len());

        let compressed_checksum_width = self
            .entries
            .iter()
            .map(|f| {
                f.compressed_checksum
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string())
                    .len()
            })
            .max()
            .unwrap_or(0)
            .max("COMPRESSED CHECKSUM".len());

        writeln!(
            f,
            "{:<index_width$} | {:<name_width$} | {:<original_size_width$} | {:<compressed_size_width$} | {:<original_checksum_width$} | {:<compressed_checksum_width$} | {:<6}",
            "INDEX",
            "NAME",
            "ORIGINAL SIZE",
            "COMPRESSED SIZE",
            "ORIGINAL CHECKSUM",
            "COMPRESSED CHECKSUM",
            "RATIO"
        )?;

        for (index, entry) in self.entries.iter().enumerate() {
            let ratio = match (entry.compressed_size, entry.source_size) {
                (Some(compressed_size), Some(source_size)) if source_size != 0 => {
                    format!("{:.1}%", (1 - (compressed_size / source_size)) * 100)
                }
                _ => "N/A".to_string(),
            };

            writeln!(
                f,
                "{:^index_width$} | {:<name_width$} | {:<original_size_width$} | {:<compressed_size_width$} | {:<original_checksum_width$} | {:<compressed_checksum_width$} | {:<6}",
                index,
                entry.name,
                entry
                    .source_size
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string()),
                entry
                    .compressed_size
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string()),
                entry
                    .source_checksum
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string()),
                entry
                    .compressed_checksum
                    .map_or_else(|| "N/A".to_string(), |c| c.to_string()),
                ratio
            )?;
        }

        Ok(())
    }
}

pub struct Header {
    pub version: [u8; 2],
    pub file_count: Option<u32>,
    pub index_offset: Option<u64>,
}
#[allow(clippy::missing_errors_doc)]
impl Header {
    pub fn decode<R: Read>(reader: &mut R) -> Result<Self> {
        let mut buffer = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buffer)?;

        if &buffer[0..4] != SIGNATURE {
            return Err(Error::IncorrectSignature(
                "<Invalid SLF signature>".to_string(),
            ));
        }

        if buffer[4] != VERSION[0] {
            return Err(Error::UnsupportedVersion {
                expected: VERSION[0],
                found: buffer[4],
            });
        }

        let version = [buffer[4], buffer[5]];
        let file_count = u32::from_be_bytes(buffer[8..12].try_into()?);

        if file_count > MAX_FILE_COUNT {
            return Err(Error::TooManyFiles {
                expected: MAX_FILE_COUNT,
                found: file_count,
            });
        }

        let file_count = Some(file_count);

        let index_offset = Some(u64::from_be_bytes(buffer[16..24].try_into()?));

        Ok(Self {
            version,
            file_count,
            index_offset,
        })
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buffer = [0u8; HEADER_SIZE];

        buffer[0..4].copy_from_slice(SIGNATURE);
        buffer[4..6].copy_from_slice(&self.version);
        buffer[8..12].copy_from_slice(&self.file_count.unwrap_or_default().to_be_bytes());
        buffer[16..24].copy_from_slice(&self.index_offset.unwrap_or_default().to_be_bytes());

        writer.write_all(&buffer)?;
        writer.flush()?;
        Ok(())
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Archive Version {}.{}\nFile count: {}",
            self.version[0],
            self.version[1],
            self.file_count
                .map_or_else(|| "N/A".to_string(), |c| c.to_string())
        )
    }
}

pub struct Entry {
    name: String,
    source_size: Option<u64>,
    compressed_size: Option<u64>,
    source_checksum: Option<u32>,
    compressed_checksum: Option<u32>,
    offset: Option<u64>,
    data_start: Option<u64>,
}
#[allow(clippy::missing_errors_doc)]
impl Entry {
    pub fn decode<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut name_len_bytes = [0u8; 4];

        reader.read_exact(&mut name_len_bytes)?;
        let name_len: usize = u32::from_be_bytes(name_len_bytes).try_into()?;

        if name_len == 0 {
            return Err(Error::EmptyFilename);
        }

        if name_len > MAX_FILENAME_SIZE {
            return Err(Error::BufferOverflow(name_len));
        }

        let mut name_bytes = vec![0u8; name_len];

        reader.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes)?;

        let mut metadata_bytes = [0u8; 24];

        reader.read_exact(&mut metadata_bytes)?;

        let source_size = Some(u64::from_be_bytes(metadata_bytes[0..8].try_into()?));
        let source_checksum = Some(u32::from_be_bytes(metadata_bytes[8..12].try_into()?));
        let compressed_size = Some(u64::from_be_bytes(metadata_bytes[12..20].try_into()?));
        let compressed_checksum = Some(u32::from_be_bytes(metadata_bytes[20..24].try_into()?));

        let data_start = Some(reader.stream_position()?);

        Ok(Self {
            name,
            source_size,
            source_checksum,
            compressed_size,
            compressed_checksum,
            offset: None,
            data_start,
        })
    }

    pub fn write<W: Write + Seek>(&mut self, mut writer: W) -> Result<()> {
        self.offset = Some(writer.stream_position()?);

        let name_len = self.name.len();
        let name_len_u32: u32 = self.name.len().try_into()?;

        let mut buffer = vec![0u8; ENTRY_SIZE + name_len];

        buffer[0..4].copy_from_slice(&name_len_u32.to_be_bytes());
        buffer[4..name_len + 4].copy_from_slice(self.name.as_bytes());
        buffer[name_len + 4..name_len + 12]
            .copy_from_slice(&self.source_size.unwrap_or_default().to_be_bytes());
        buffer[name_len + 12..name_len + 16]
            .copy_from_slice(&self.source_checksum.unwrap_or_default().to_be_bytes());
        buffer[name_len + 16..name_len + 24]
            .copy_from_slice(&self.compressed_size.unwrap_or_default().to_be_bytes());
        buffer[name_len + 24..name_len + 28]
            .copy_from_slice(&self.compressed_checksum.unwrap_or_default().to_be_bytes());

        writer.write_all(&buffer)?;

        self.data_start = Some(writer.stream_position()?);

        writer.flush()?;

        Ok(())
    }

    pub fn update<W: Write + Seek>(&self, mut writer: W) -> Result<()> {
        let name_len: u64 = self.name.len().try_into()?;

        let empty_fields_offset = self.offset.ok_or(Error::Empty(String::from(
            "Can't get offset from file entry",
        )))? + 4
            + name_len
            + 8;

        writer.seek(SeekFrom::Start(empty_fields_offset))?;

        let mut buffer = [0u8; 16];
        buffer[0..4].copy_from_slice(
            &self
                .source_checksum
                .ok_or(Error::Empty(String::from(
                    "Can't get source checksum from file entry",
                )))?
                .to_be_bytes(),
        );
        buffer[4..12].copy_from_slice(
            &self
                .compressed_size
                .ok_or(Error::Empty(String::from(
                    "Can't get compressed size from file entry",
                )))?
                .to_be_bytes(),
        );
        buffer[12..16].copy_from_slice(
            &self
                .compressed_checksum
                .ok_or(Error::Empty(String::from(
                    "Can't get compressed checksum from file entry",
                )))?
                .to_be_bytes(),
        );
        writer.write_all(&buffer)?;

        writer.seek(SeekFrom::End(0))?;

        writer.flush()?;
        Ok(())
    }
}

#[allow(clippy::missing_errors_doc)]
pub fn extraction_path(source: &Path, target: &Path) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || !source.is_file() || source.extension().is_none_or(|ex| ex != "slf") {
        return Err(Error::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }

    let extraction_path = if target.is_file() {
        return Err(Error::Path(format!(
            "Archive can't be unpacked into file at path: {}",
            target.display(),
        )));
    } else {
        let source_stem = source.file_stem().ok_or(Error::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?;
        target.join(source_stem)
    };

    Ok(extraction_path)
}

#[allow(clippy::missing_errors_doc)]
pub fn archive_path(source: &Path, target: &Path) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || (!source.is_file() && !source.is_dir()) {
        return Err(Error::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }
    Ok(if target.extension().is_some_and(|ex| ex == "slf") {
        target
    } else {
        let archive_name = archive_name(&source)?;
        target.join(archive_name).with_extension("slf")
    })
}

fn archive_name(source: &Path) -> Result<PathBuf> {
    Ok(if source.is_file() {
        PathBuf::from(source.file_stem().ok_or(Error::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?)
    } else {
        PathBuf::from(source.file_name().ok_or(Error::Path(format!(
            "Failed to get directory name from path: {}",
            source.display()
        )))?)
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => {
                normalized.clear();
                normalized.push(Component::Prefix(p));
            }
            Component::RootDir => {
                normalized.clear();
                normalized.push(Component::RootDir);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = normalized.last() {
                    if let Component::Normal(_) = last {
                        normalized.pop();
                    }
                } else {
                    normalized.push(component);
                }
            }
            Component::Normal(_) => normalized.push(component),
        }
    }
    normalized.iter().collect()
}

fn safe_join(base: &Path, untrusted: &Path) -> Result<PathBuf> {
    let sanitized: PathBuf = untrusted
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .collect();

    if sanitized.as_os_str().is_empty() {
        return Err(Error::Path(format!(
            "Path sanitized to nothing: {}",
            untrusted.display()
        )));
    }

    let result = base.join(sanitized);

    if !result.starts_with(base) {
        return Err(Error::Path(format!(
            "path traversal detected: {}",
            untrusted.display()
        )));
    }

    Ok(result)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionType {
    Smart,
    Force,
    None,
}
