use std::{
    fmt::Display,
    fs::create_dir_all,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use flate2::{Crc, write::GzDecoder};
use tempfile::NamedTempFile;

use crate::{
    ArchiveReader, BUFFER_SIZE, Error, Result,
    archive::{HasherWriter, entry::Entry, header::Header},
    utils::safe_join,
};

#[allow(clippy::missing_errors_doc)]
impl<R: Read + Seek> ArchiveReader<R> {
    pub fn open(mut reader: R) -> Result<Self> {
        let header = Header::decode(&mut reader)?;

        reader.seek(SeekFrom::Start(header.index_offset))?;

        let mut entry_offsets: Vec<u64> = Vec::with_capacity(header.file_count.try_into()?);
        for _ in 0..header.file_count {
            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            entry_offsets.push(u64::from_be_bytes(offset_bytes));
        }

        let mut entries: Vec<Entry> = Vec::with_capacity(header.file_count.try_into()?);
        for offset in entry_offsets {
            reader.seek(SeekFrom::Start(offset))?;
            let entry = Entry::decode(&mut reader)?;
            entries.push(entry);
        }

        Ok(Self {
            reader,
            header,
            entries,
        })
    }

    pub fn extract(&mut self, index: u32, target: &Path) -> Result<()> {
        let entry = self.get_entry(index)?.clone();
        Self::unpack_entry(&mut self.reader, &entry, self.header.index_offset, target)
    }

    pub fn extract_all(&mut self, target: &Path) -> Result<()> {
        for i in 0..self.header.file_count {
            self.extract(i, target)?;
        }
        Ok(())
    }

    fn get_entry(&self, index: u32) -> Result<&Entry> {
        self.entries
            .get(index as usize)
            .ok_or(Error::IndexOutOfRange {
                index,
                file_count: self.entries.len().try_into()?,
            })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn index_offset(&self) -> u64 {
        self.header.index_offset
    }
}

impl<W> ArchiveReader<W> {
    pub fn unpack_entry<T: Read + Seek>(
        reader: &mut T,
        entry: &Entry,
        index_offset: u64,
        target: &Path,
    ) -> Result<()> {
        let path = safe_join(target, Path::new(&entry.name))?;

        if let Some(parents) = path.parent() {
            create_dir_all(parents)?;
        }

        let parent = path.parent().unwrap_or(target);
        let temp_file = NamedTempFile::new_in(parent)?;

        let stats = Self::decompress_entry(reader, entry, index_offset, &temp_file)?;

        Self::verify_entry(entry, &stats)?;

        temp_file.persist(&path).map_err(|e| Error::Io(e.error))?;

        Ok(())
    }

    fn decompress_entry<T: Read + Seek>(
        reader: &mut T,
        entry: &Entry,
        index_offset: u64,
        temp_file: &NamedTempFile,
    ) -> Result<DecompressStats> {
        let mut writer = BufWriter::new(temp_file);

        let mut hasher_writer = HasherWriter::new(&mut writer);

        let mut compressed_checksum = Crc::new();

        let mut decoder = GzDecoder::new(&mut hasher_writer);

        let mut remaining_bytes = entry.compressed_size;

        if entry
            .data_start
            .checked_add(entry.compressed_size)
            .ok_or(Error::IncorrectEntry(String::from(
                "too big data_start and/or compressed size",
            )))?
            > index_offset
        {
            return Err(Error::IncorrectEntry(String::from(
                "start of data + compressed size greater than index offset",
            )));
        }

        let mut buffer = vec![0u8; BUFFER_SIZE];

        reader.seek(SeekFrom::Start(entry.data_start))?;

        loop {
            if remaining_bytes == 0 {
                break;
            }

            let to_read = usize::try_from(remaining_bytes.min(buffer.len() as u64))?;

            let bytes = reader.read(&mut buffer[..to_read])?;

            if bytes == 0 {
                break;
            }

            let chunk = &buffer[..bytes];

            compressed_checksum.update(chunk);

            decoder.write_all(chunk)?;

            remaining_bytes -= bytes as u64;
        }

        if remaining_bytes != 0 {
            return Err(Error::SizeMismatch {
                expected: entry.compressed_size,
                found: entry.compressed_size - remaining_bytes,
            });
        }

        decoder.finish()?;

        let source_checksum = hasher_writer.sum();
        let compressed_checksum = compressed_checksum.sum();
        let source_size = hasher_writer.take_and_reset_bytes();

        writer.flush()?;

        Ok(DecompressStats {
            source_checksum,
            compressed_checksum,
            source_size,
        })
    }

    fn verify_entry(entry: &Entry, stats: &DecompressStats) -> Result<()> {
        if stats.source_checksum != entry.source_checksum {
            return Err(Error::ChecksumMismatch {
                expected: entry.source_checksum,
                found: stats.source_checksum,
            });
        }

        if stats.compressed_checksum != entry.compressed_checksum {
            return Err(Error::ChecksumMismatch {
                expected: entry.compressed_checksum,
                found: stats.compressed_checksum,
            });
        }

        if entry.source_size != stats.source_size {
            return Err(Error::SizeMismatch {
                expected: entry.source_size,
                found: stats.source_size,
            });
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
            .map(|f| f.source_size.to_string().len())
            .max()
            .unwrap_or(0)
            .max("ORIGINAL SIZE".len());
        let compressed_size_width = self
            .entries
            .iter()
            .map(|f| f.compressed_size.to_string().len())
            .max()
            .unwrap_or(0)
            .max("COMPRESSED SIZE".len());

        let original_checksum_width = self
            .entries
            .iter()
            .map(|f| f.source_checksum.to_string().len())
            .max()
            .unwrap_or(0)
            .max("ORIGINAL CHECKSUM".len());

        let compressed_checksum_width = self
            .entries
            .iter()
            .map(|f| f.compressed_checksum.to_string().len())
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
            "SAVED"
        )?;

        for (index, entry) in self.entries.iter().enumerate() {
            let ratio = if entry.source_size != 0 {
                format!(
                    "{}%",
                    100u64.saturating_sub(entry.compressed_size * 100 / entry.source_size)
                )
            } else {
                "N/A".to_string()
            };

            writeln!(
                f,
                "{:^index_width$} | {:<name_width$} | {:<original_size_width$} | {:<compressed_size_width$} | {:<original_checksum_width$} | {:<compressed_checksum_width$} | {:<6}",
                index,
                entry.name,
                entry.source_size,
                entry.compressed_size,
                entry.source_checksum,
                entry.compressed_checksum,
                ratio
            )?;
        }

        Ok(())
    }
}

struct DecompressStats {
    source_checksum: u32,
    compressed_checksum: u32,
    source_size: u64,
}
