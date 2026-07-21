use std::{
    collections::HashMap,
    fmt::Display,
    fs::{File, create_dir_all},
    io::{Read, Seek, SeekFrom, Write},
    num::NonZero,
    path::Path,
    thread,
};

use crc32fast::Hasher;
use tempfile::NamedTempFile;

use crate::{
    ArchiveReader, BUFFER_SIZE, Error, MAX_FILE_SOURCE_SIZE, Result,
    archive::{HasherWriter, entry::Entry, header::Header},
    utils::safe_join,
};

#[allow(clippy::missing_errors_doc)]
impl<R: Read + Seek> ArchiveReader<R> {
    pub fn open(mut reader: R) -> Result<Self> {
        let header = Header::decode(&mut reader)?;

        reader.seek(SeekFrom::Start(header.index_offset))?;

        let mut entry_offsets: Vec<u64> = Vec::new();
        for _ in 0..header.file_count {
            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            entry_offsets.push(u64::from_be_bytes(offset_bytes));
        }

        let mut entries: Vec<Entry> = Vec::new();
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

    pub fn extract_all(&mut self, source: &Path, target: &Path) -> Result<()> {
        if self.entries().is_empty() {
            return Ok(());
        }

        let threads_num = std::thread::available_parallelism().map_or(4, NonZero::get);

        let index_offset = self.index_offset();

        thread::scope(|s| -> Result<()> {
            let chunk_size = self.header.file_count.div_ceil(threads_num.try_into()?);
            let chunks = self.entries().chunks(chunk_size.try_into()?);

            let mut handles = Vec::new();

            for chunk in chunks {
                let handle = s.spawn(move || -> Result<()> {
                    let mut file = File::open(source)?;
                    for entry in chunk {
                        Self::unpack_entry(&mut file, entry, index_offset, target)?;
                    }
                    Ok(())
                });

                handles.push(handle);
            }

            for handle in handles {
                handle.join().map_err(|_| Error::ThreadPanic)??;
            }
            Ok(())
        })?;
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

    pub fn get_entries_map(&self) -> Result<HashMap<&str, u32>> {
        let mut entries_map: HashMap<&str, u32> = HashMap::new();

        for index in 0..self.entries().len() {
            let entry_name = &self
                .entries
                .get(index)
                .ok_or(Error::IndexOutOfRange {
                    index: index.try_into()?,
                    file_count: self.entries.len().try_into()?,
                })?
                .name;

            entries_map.insert(entry_name, index.try_into()?);
        }

        Ok(entries_map)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn index_offset(&self) -> u64 {
        self.header.index_offset
    }
}

impl<D> ArchiveReader<D> {
    #[allow(clippy::missing_errors_doc)]
    pub fn unpack_entry<T: Read + Seek>(
        reader: &mut T,
        entry: &Entry,
        index_offset: u64,
        target: &Path,
    ) -> Result<()> {
        let path = safe_join(target, Path::new(&entry.name))?;

        for ancestors in path.ancestors() {
            if ancestors.exists() && ancestors.starts_with(target) && ancestors != target {
                let metadata = ancestors.symlink_metadata()?;
                if metadata.is_symlink() {
                    return Err(Error::Path(format!(
                        "symlink detected at destination path: {}",
                        ancestors.display()
                    )));
                }
            }
        }

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

    fn decompress_entry<T: Read + Seek, W: Write>(
        reader: &mut T,
        entry: &Entry,
        index_offset: u64,
        mut writer: W,
    ) -> Result<DecompressStats> {
        let mut hasher_writer = HasherWriter::new(&mut writer);

        let mut compressed_checksum = Hasher::new();

        let mut decoder = zstd::stream::write::Decoder::new(&mut hasher_writer)?;

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

            let decoded_bytes = decoder.get_ref().bytes;

            if decoded_bytes > MAX_FILE_SOURCE_SIZE || decoded_bytes > entry.source_size {
                return Err(Error::SizeMismatch {
                    expected: entry.source_size,
                    found: decoded_bytes,
                });
            }

            remaining_bytes -= bytes as u64;
        }

        if remaining_bytes != 0 {
            return Err(Error::SizeMismatch {
                expected: entry.compressed_size,
                found: entry.compressed_size - remaining_bytes,
            });
        }

        decoder.flush()?;

        drop(decoder);

        let source_checksum = hasher_writer.sum();
        let compressed_checksum = compressed_checksum.finalize();
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

impl<D> Display for ArchiveReader<D> {
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
            entry
                .compressed_size
                .checked_mul(100)
                .expect("Compressed size of file is too big");

            let ratio = if entry.source_size != 0 {
                format!(
                    "{}%",
                    100u64.saturating_sub(
                        (entry.compressed_size * 100)
                            .checked_div(entry.source_size)
                            .expect("Can't divide by source size")
                    )
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
