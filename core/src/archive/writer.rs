use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Seek, SeekFrom, Write},
    num::NonZero,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use crate::{
    ArchiveWriter, HEADER_SIZE, Result, VERSION,
    archive::{CompressedFile, header::Header},
    utils::collect_files,
};

#[allow(clippy::missing_errors_doc)]
impl<W: Write + Seek> ArchiveWriter<W> {
    pub fn new(writer: W) -> Result<Self> {
        let header = Header {
            version: VERSION,
            file_count: 0,
            index_offset: 0,
        };

        Ok(Self {
            writer,
            header,
            entries: Vec::new(),
            compression_level: 3,
        })
    }
    #[must_use]
    pub fn with_compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    pub fn pack(mut self, source: &Path) -> Result<(W, PackStats)> {
        let mut file_paths = collect_files(source)?;
        file_paths.sort();

        if file_paths.is_empty() {
            self.header.file_count = 0;
            self.header.index_offset = 24;

            self.writer.seek(SeekFrom::Start(0))?;
            self.header.write(&mut self.writer)?;
            self.writer.flush()?;

            let pack_stats = PackStats {
                source_size: 0,
                compressed_size: 0,
                file_count: 0,
            };

            return Ok((self.writer, pack_stats));
        }

        self.write_files(file_paths)?;

        self.header.file_count = self.entries.len().try_into()?;

        let index_offset = self.writer.stream_position()?;

        for entry in &self.entries {
            let offset = entry.offset;
            self.writer.write_all(&offset.to_be_bytes())?;
        }

        self.header.index_offset = index_offset;
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        self.writer.flush()?;

        let pack_stats = PackStats {
            source_size: self.entries.iter().map(|e| e.source_size).sum(),
            compressed_size: self.entries.iter().map(|e| e.compressed_size).sum(),
            file_count: self.header.file_count,
        };

        Ok((self.writer, pack_stats))
    }

    fn write_files(&mut self, file_paths: Vec<(PathBuf, String)>) -> Result<()> {
        let indexed_files: Vec<((PathBuf, String), u32)> =
            file_paths.into_iter().zip(0u32..).collect();

        let threads_num = std::thread::available_parallelism().map_or(4, NonZero::get);

        let compression_level = self.compression_level;

        let (tx, rx) = mpsc::sync_channel::<Result<CompressedFile>>(threads_num);

        let mut pending = BTreeMap::new();
        let mut next_id_to_write = 0;

        self.writer.seek(SeekFrom::Start(HEADER_SIZE.try_into()?))?;

        thread::scope(|s| -> Result<()> {
            let chunk_size = indexed_files.len().div_ceil(threads_num);

            let chunks = indexed_files.chunks(chunk_size);

            for chunk in chunks {
                let tx = tx.clone();
                s.spawn(move || -> Result<()> {
                    for ((path, name), id) in chunk {
                        if tx
                            .send(CompressedFile::create(
                                *id,
                                path,
                                name.clone(),
                                compression_level,
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(())
                });
            }

            drop(tx);

            for respond in rx {
                let compressed_file = respond?;
                pending.insert(compressed_file.id, compressed_file);

                while let Some(mut file) = pending.remove(&next_id_to_write) {
                    file.entry.write(&mut self.writer)?;

                    let content_file = File::open(&file.content_path)?;
                    let mut reader = BufReader::new(content_file);
                    std::io::copy(&mut reader, &mut self.writer)?;

                    self.entries.push(file.entry);

                    next_id_to_write += 1;
                }
            }

            Ok(())
        })?;

        Ok(())
    }
}

pub struct PackStats {
    pub source_size: u64,
    pub compressed_size: u64,
    pub file_count: u32,
}
