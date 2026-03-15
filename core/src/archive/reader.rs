use std::{
    fmt::Display,
    fs::{File, create_dir_all},
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use flate2::{Crc, write::GzDecoder};

use crate::{
    ArchiveReader, BUFFER_SIZE, Error, Result,
    archive::{entry::Entry, header::Header, writer::HasherWriter},
    utils::safe_join,
};

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
