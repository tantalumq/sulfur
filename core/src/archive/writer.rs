use std::{
    io::{BufReader, Seek, SeekFrom, Write},
    path::Path,
};

use crate::{
    ArchiveWriter, Result, VERSION,
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
        })
    }

    pub fn pack(self, source: &Path) -> Result<W> {
        let mut file_paths = collect_files(source)?;
        file_paths.sort();

        let compressed_files = file_paths
            .iter()
            .map(|(path, name)| CompressedFile::create(path, name.clone()))
            .collect::<Result<Vec<CompressedFile>>>()?;

        self.assemble(compressed_files)
    }
    fn assemble(mut self, compressed_files: Vec<CompressedFile>) -> Result<W> {
        self.header.file_count = compressed_files.len().try_into()?;
        self.writer.seek(SeekFrom::Start(0))?;
        self.header.write(&mut self.writer)?;

        for mut file in compressed_files {
            file.entry.write(&mut self.writer)?;

            let mut reader = BufReader::new(file.content.reopen()?);
            std::io::copy(&mut reader, &mut self.writer)?;

            self.entries.push(file.entry);
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
