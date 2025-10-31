use std::{
    fs::{File, create_dir},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use flate2::{Crc, write::GzDecoder};

use crate::{
    BUFFER_SIZE, HasherWriter, InnerFile, SIGNATURE,
    error::{ArchiveError, Result},
};

pub fn unpack(archive: &Path) -> Result<()> {
    let str_path = archive
        .to_str()
        .ok_or(ArchiveError::Path(format!("Can't read path")))?;

    if !archive.exists() {
        return Err(ArchiveError::Path(format!(
            "Archive doesn't exists at this path: {}",
            str_path
        )));
    }

    archive.extension().ok_or(ArchiveError::Path(format!(
        "Incorrect file type at path: {}",
        str_path
    )))?;

    let file = File::open(archive)?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; BUFFER_SIZE];

    reader.read_exact(&mut buffer[..4])?;
    if &buffer[..4] != SIGNATURE {
        return Err(ArchiveError::Path(format!(
            "File is corrupted or have incorrect type. File at path: {}",
            str_path
        )));
    }

    reader.read_exact(&mut buffer[..4])?;
    let file_count = u32::from_le_bytes(buffer[..4].try_into()?);

    let dir_pathbuf = archive.with_extension("");
    let dir_path = dir_pathbuf.as_path();

    if file_count > 1 {
        create_dir(dir_path)?;
    }

    for _ in 0..file_count {
        let inner_file = InnerFile::from_archive(&mut reader, &mut buffer)?;

        let file_path = if file_count > 1 {
            dir_path.join(&inner_file.name)
        } else {
            PathBuf::from(&inner_file.name)
        };
        let file = File::create(file_path)?;

        let mut writer = BufWriter::new(file);

        let position = reader.stream_position()?;
        reader.seek(SeekFrom::Start(inner_file.offset))?;

        let mut compressed_checksum = Crc::new();

        let hasher = Crc::new();
        let mut hasher_writer = HasherWriter::new(&mut writer, hasher);

        let mut decoder = GzDecoder::new(&mut hasher_writer);

        let mut remaining_bytes = inner_file.compressed_size;

        loop {
            let to_read = remaining_bytes.min(BUFFER_SIZE as u64) as usize;

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

        reader.seek(SeekFrom::Start(position))?;

        if original_checksum != inner_file.original_checksum {
            todo!("original cheksums aren't equal")
        }

        if compressed_checksum != inner_file.compressed_checksum {
            todo!("compressed cheksums aren't equal")
        }

        if inner_file.original_size != hasher_writer.take_written_bytes() {
            todo!("original file size and unpacked file size aren't equal")
        }
    }

    Ok(())
}
