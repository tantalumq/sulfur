use crate::{
    BUFFER_SIZE, Result, error::ArchiveError, get_extraction_path, unpack::unpack_file,
    validate_archive,
};
use std::{
    fs::{File, create_dir_all},
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

#[allow(clippy::missing_errors_doc)]
pub fn get(source: &Path, target: &Path, index: u32) -> Result<()> {
    let extraction_path = get_extraction_path(source, target)?;

    let file = File::open(source)?;
    let mut reader = BufReader::new(file);

    let mut buffer = vec![0u8; BUFFER_SIZE];

    validate_archive(&mut reader, &mut buffer[..], source)?;

    reader.read_exact(&mut buffer[..2])?; // skip padding

    reader.read_exact(&mut buffer[..4])?;
    let file_count = u32::from_be_bytes(buffer[..4].try_into()?);

    if index >= file_count {
        return Err(ArchiveError::IndexOutOfRange { index, file_count });
    }

    reader.read_exact(&mut buffer[..4])?; // skip padding

    reader.read_exact(&mut buffer[..8])?;
    let index_offset = u64::from_be_bytes(buffer[..8].try_into()?);

    reader.seek(SeekFrom::Start(index_offset + u64::from(index * 8)))?;

    reader.read_exact(&mut buffer[..8])?;
    let file_offset = u64::from_be_bytes(buffer[..8].try_into()?);

    reader.seek(SeekFrom::Start(file_offset))?;

    if let Some(parents) = extraction_path.parent() {
        create_dir_all(parents)?;
    }

    unpack_file(&mut reader, &extraction_path, &mut buffer)?;

    Ok(())
}
