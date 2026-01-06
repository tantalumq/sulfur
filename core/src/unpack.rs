use std::{
    fs::{File, create_dir_all},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

use flate2::{Crc, write::GzDecoder};

use crate::{
    BUFFER_SIZE, HasherWriter, InnerFile,
    error::{ArchiveError, Result},
    get_extraction_path, safe_join, validate_archive,
};

#[allow(clippy::missing_errors_doc)]
pub fn unpack(source: &Path, target: &Path) -> Result<()> {
    let extraction_path = get_extraction_path(source, target)?;

    let file = File::open(source)?;
    let mut reader = BufReader::new(file);

    let mut buffer = vec![0u8; BUFFER_SIZE];

    validate_archive(&mut reader, &mut buffer[..], source)?;

    reader.read_exact(&mut buffer[..4])?;
    let file_count = u32::from_be_bytes(buffer[..4].try_into()?);

    reader.read_exact(&mut buffer[..8])?; // skip index offset

    let dir_path = {
        let source_stem = source.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?;
        extraction_path.join(source_stem)
    };

    if let Some(parents) = dir_path.parent() {
        create_dir_all(parents)?;
    }

    for _ in 0..file_count {
        unpack_file(&mut reader, &dir_path, &mut buffer)?;
    }
    Ok(())
}

#[allow(clippy::missing_errors_doc)]
pub fn unpack_file(reader: &mut BufReader<File>, dir_path: &Path, buffer: &mut [u8]) -> Result<()> {
    let inner_file = InnerFile::from_archive(reader, buffer)?;

    let file_path = safe_join(dir_path, Path::new(&inner_file.name))?;

    if let Some(parents) = file_path.parent() {
        create_dir_all(parents)?;
    }

    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    let hasher = Crc::new();
    let mut hasher_writer = HasherWriter::new(&mut writer, hasher);

    let (original_checksum, compressed_checksum) =
        unpack_single_file(&inner_file, reader, &mut hasher_writer, buffer)?;

    if original_checksum != inner_file.original_checksum {
        return Err(ArchiveError::ChecksumMismatch {
            expected: inner_file.original_checksum,
            found: original_checksum,
        });
    }

    if compressed_checksum != inner_file.compressed_checksum {
        return Err(ArchiveError::ChecksumMismatch {
            expected: inner_file.compressed_checksum,
            found: compressed_checksum,
        });
    }

    let size = hasher_writer.take_and_reset_bytes();
    if inner_file.original_size != size {
        return Err(ArchiveError::SizeMismatch {
            expected: inner_file.original_size,
            found: size,
        });
    }

    Ok(())
}

fn unpack_single_file(
    inner_file: &InnerFile,
    reader: &mut BufReader<File>,
    mut hasher_writer: &mut HasherWriter,
    buffer: &mut [u8],
) -> Result<(u32, u32)> {
    let mut compressed_checksum = Crc::new();

    let mut decoder = GzDecoder::new(&mut hasher_writer);

    let mut remaining_bytes = inner_file.compressed_size;

    loop {
        let to_read = usize::try_from(remaining_bytes.min(BUFFER_SIZE as u64))?;

        let bytes = reader.read(&mut buffer[..to_read])?;

        if bytes == 0 {
            break;
        }

        let chunk = &buffer[..bytes];

        compressed_checksum.update(chunk);

        decoder.write_all(chunk)?;

        remaining_bytes -= bytes as u64;
    }

    decoder.finish()?;

    let original_checksum = hasher_writer.sum();
    let compressed_checksum = compressed_checksum.sum();
    Ok((original_checksum, compressed_checksum))
}
