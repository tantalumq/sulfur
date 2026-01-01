use std::{
    fs::{File, create_dir_all},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use flate2::{Crc, write::GzDecoder};

use crate::{
    BUFFER_SIZE, HasherWriter, InnerFile, SIGNATURE, VERSION,
    error::{ArchiveError, Result},
    normalize_path,
};

#[allow(clippy::missing_errors_doc)]
pub fn unpack(source: &Path, target: &Path) -> Result<()> {
    let extraction_path = get_extraction_path(source, target)?;

    let file = File::open(source)?;
    let mut reader = BufReader::new(file);

    #[allow(clippy::large_stack_arrays)]
    let mut buffer = [0u8; BUFFER_SIZE];

    validate_archive(&mut reader, &mut buffer, source)?;

    reader.read_exact(&mut buffer[..4])?;
    let file_count = u32::from_be_bytes(buffer[..4].try_into()?);

    reader.read_exact(&mut buffer[..8])?; // skip index offset

    let dir_path = if file_count > 1 {
        let source_stem = source.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?;
        extraction_path.join(source_stem)
    } else {
        extraction_path
    };

    if let Some(parents) = dir_path.parent() {
        create_dir_all(parents)?;
    }

    unpack_files(&mut reader, file_count, &dir_path, &mut buffer)?;

    Ok(())
}

#[allow(clippy::missing_errors_doc)]
pub fn validate_archive(
    reader: &mut BufReader<File>,
    buffer: &mut [u8],
    path: &Path,
) -> Result<()> {
    reader.read_exact(&mut buffer[..4])?;
    if &buffer[..4] != SIGNATURE {
        return Err(ArchiveError::Path(format!(
            "File is corrupted or has incorrect type. File at path: {}",
            path.display()
        )));
    }

    reader.read_exact(&mut buffer[..2])?;
    if buffer[0] != VERSION[0] {
        return Err(ArchiveError::Path(format!(
            "File is corrupted or has incorrect type. File at path: {}",
            path.display()
        )));
    }
    Ok(())
}

fn unpack_files(
    reader: &mut BufReader<File>,
    file_count: u32,
    dir_path: &Path,
    buffer: &mut [u8],
) -> Result<()> {
    for _ in 0..file_count {
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
            return Err(ArchiveError::CorruptedArchive(format!(
                "Archive corrupted! Unpacked checksums isn't equal to! {} isn't equal to {}",
                original_checksum, inner_file.original_checksum
            )));
        }

        if compressed_checksum != inner_file.compressed_checksum {
            return Err(ArchiveError::CorruptedArchive(format!(
                "Archive corrupted! Unpacked checksums isn't equal to! {} isn't equal to {}",
                compressed_checksum, inner_file.compressed_checksum
            )));
        }

        let size = hasher_writer.take_written_bytes();
        if inner_file.original_size != size {
            return Err(ArchiveError::CorruptedArchive(format!(
                "Archive corrupted! Unpacked file has another size! {} isn't equal to {}",
                inner_file.original_size, size
            )));
        }
    }
    Ok(())
}

fn safe_join(base: &Path, untrusted: &Path) -> Result<PathBuf> {
    let cleaned: PathBuf = untrusted
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n),
            _ => None,
        })
        .collect();

    if cleaned.as_os_str().is_empty() {
        return Err(ArchiveError::Path(format!(
            "Path sanitazed to nothing: {}",
            untrusted.display()
        )));
    }

    let result = base.join(&cleaned);

    if !result.starts_with(base) {
        return Err(ArchiveError::Path(format!(
            "path traversal detected: {}",
            untrusted.display()
        )));
    }

    Ok(result)
}

fn get_extraction_path(source: &Path, target: &Path) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || !source.is_file() || source.extension().is_none_or(|ex| ex != "slf") {
        return Err(ArchiveError::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }

    Ok(if target.is_file() {
        return Err(ArchiveError::Path(format!(
            "Archive can't be unpacked into file at path: {}",
            target.display(),
        )));
    } else {
        target
    })
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
