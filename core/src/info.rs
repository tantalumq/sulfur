use std::{
    fmt::Display,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{BUFFER_SIZE, InnerFile, error::Result, validate_archive};

#[derive(Debug)]
pub struct ArchiveInfo {
    version: [u8; 2],
    file_count: u32,
    files: Vec<InnerFile>,
}

impl Display for ArchiveInfo {
    #[allow(clippy::cast_precision_loss)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Archive Version {}.{}\nFile count: {}",
            self.version[0], self.version[1], self.file_count,
        )?;

        let index_width = self.files.len().to_string().len().max("INDEX".len());
        let name_width = self.files.iter().map(|f| f.name.len()).max().unwrap_or(0);
        let original_size_width = self
            .files
            .iter()
            .map(|f| f.original_size.to_string().len())
            .max()
            .unwrap_or(0)
            .max("ORIGINAL SIZE".len());
        let compressed_size_width = self
            .files
            .iter()
            .map(|f| f.compressed_size.to_string().len())
            .max()
            .unwrap_or(0)
            .max("COMPRESSED SIZE".len());

        let original_checksum_width = self
            .files
            .iter()
            .map(|f| f.original_checksum.to_string().len())
            .max()
            .unwrap_or(0)
            .max("ORIGINAL CHECKSUM".len());

        let compressed_checksum_width = self
            .files
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
            "RATIO"
        )?;

        for (index, file) in self.files.iter().enumerate() {
            writeln!(
                f,
                "{:^index_width$} | {:<name_width$} | {:<original_size_width$} | {:<compressed_size_width$} | {:<original_checksum_width$} | {:<compressed_checksum_width$} | {:<6.1}%",
                index,
                file.name.display(),
                file.original_size,
                file.compressed_size,
                file.original_checksum,
                file.compressed_checksum,
                (1.0 - (file.compressed_size as f64 / file.original_size as f64)) * 100.0
            )?;
        }
        Ok(())
    }
}

#[allow(clippy::missing_errors_doc)]
pub fn info(source: &Path) -> Result<ArchiveInfo> {
    let file = File::open(source)?;
    let mut reader = BufReader::new(file);

    let mut buffer = vec![0u8; BUFFER_SIZE];

    validate_archive(&mut reader, &mut buffer[..], source)?;

    let version = buffer[..2].try_into()?;

    reader.read_exact(&mut buffer[..2])?; // skip padding

    reader.read_exact(&mut buffer[..4])?;
    let file_count = u32::from_be_bytes(buffer[0..4].try_into()?);

    reader.read_exact(&mut buffer[..4])?; // skip padding

    reader.read_exact(&mut buffer[..8])?;
    let index_offset = u64::from_be_bytes(buffer[..8].try_into()?);

    reader.seek(SeekFrom::Start(index_offset))?;

    let index_array_size = 8 * file_count as usize;

    reader.read_exact(&mut buffer[..index_array_size])?;
    let file_offsets = buffer[..index_array_size]
        .chunks(8)
        .map(|o| -> Result<u64> { Ok(u64::from_be_bytes(o.try_into()?)) })
        .collect::<Result<Vec<u64>>>()?;

    let mut files = vec![];
    for offset in file_offsets {
        reader.seek(SeekFrom::Start(offset))?;
        files.push(InnerFile::from_archive(&mut reader, &mut buffer)?);
    }
    Ok(ArchiveInfo {
        version,
        file_count,
        files,
    })
}
