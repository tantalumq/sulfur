use std::{
    ffi::OsString,
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use flate2::{Compression, Crc, write::GzEncoder};
use walkdir::WalkDir;

use crate::{
    error::{ArchiveError, Result},
    validate_path,
};

use crate::{BUFFER_SIZE, HasherWriter, InnerFile, SIGNATURE};

pub fn pack(root: &Path) -> Result<()> {
    let archive_path = archive_path(root)?;
    let file = File::create(archive_path)?;
    let mut writer = BufWriter::new(file);

    let files: Vec<PathBuf> = collect_files(root);

    writer.write_all(SIGNATURE)?;
    writer.write_all(&(files.len() as u32).to_le_bytes())?;

    let inners = inner_files(root, &files)?;

    let mut temp_offsets = Vec::new();

    for inner in inners {
        temp_offsets.push(inner.write_metadata(&mut writer)?);
    }

    let (data_offsets, compressed_sizes, checksums) = process_files(files, &mut writer)?;

    writer.flush()?;

    rewrite_temp_fields(
        &mut writer,
        temp_offsets,
        data_offsets,
        compressed_sizes,
        checksums,
    )?;

    writer.flush()?;
    Ok(())
}

fn archive_path(root: &Path) -> Result<OsString> {
    let os_path = validate_path(root)?;

    let archive_name = if root.is_file() {
        root.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            os_path.display()
        )))?
    } else {
        root.file_name().ok_or(ArchiveError::Path(format!(
            "Failed to get directory name from path: {}",
            os_path.display()
        )))?
    };

    Ok(PathBuf::from(archive_name)
        .with_extension("slf")
        .into_os_string())
}

fn collect_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        vec![root.to_path_buf()]
    } else {
        WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect()
    }
}

fn inner_files(root: &Path, paths: &Vec<PathBuf>) -> Result<Vec<InnerFile>> {
    let mut inners = Vec::new();

    for path in paths {
        let relative_name = if root.is_file() {
            path.file_name()
                .ok_or(ArchiveError::Path(format!(
                    "Failed to get file name from path: {}",
                    root.display()
                )))?
                .to_os_string()
        } else {
            path.strip_prefix(root)?.as_os_str().to_os_string()
        };
        let file_size = path.metadata()?.len();

        let inner_file = InnerFile::create(relative_name, file_size, 0, 0, 0, 0);

        inners.push(inner_file);
    }
    Ok(inners)
}

fn process_files(
    paths: Vec<PathBuf>,
    writer: &mut BufWriter<File>,
) -> Result<(Vec<u64>, Vec<u64>, Vec<(u32, u32)>)> {
    let mut data_offsets = Vec::new();
    let mut compressed_sizes = Vec::new();
    let mut checksums = Vec::new();

    for path in paths {
        let hasher = Crc::new();
        let hasher_writer = HasherWriter::new(writer, hasher);

        let (offset, size, (original_cheksum, compressed_checksum)) =
            process_single_file(path, hasher_writer)?;

        data_offsets.push(offset);
        compressed_sizes.push(size);
        checksums.push((original_cheksum, compressed_checksum));
    }

    Ok((data_offsets, compressed_sizes, checksums))
}

fn process_single_file(
    path: PathBuf,
    mut hasher_writer: HasherWriter,
) -> Result<(u64, u64, (u32, u32))> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut buffer = [0u8; BUFFER_SIZE];

    let mut original_checksum = Crc::new();
    let offset = hasher_writer.stream_position()?;

    let encoder = GzEncoder::new(hasher_writer, Compression::default());

    hasher_writer =
        compress_file_content(&mut reader, encoder, &mut original_checksum, &mut buffer)?;

    let size = hasher_writer.take_written_bytes();

    let original_checksum = original_checksum.sum();
    let compressed_checksum = hasher_writer.sum();

    Ok((offset, size, (original_checksum, compressed_checksum)))
}

fn compress_file_content<R: Read, W: Write>(
    reader: &mut R,
    mut encoder: GzEncoder<W>,
    checksum: &mut Crc,
    mut buffer: &mut [u8],
) -> Result<W> {
    loop {
        let bytes = reader.read(&mut buffer)?;

        if bytes == 0 {
            break; //EOF
        }

        let chunk = &buffer[..bytes];

        checksum.update(&chunk);

        encoder.write_all(chunk)?;
    }

    Ok(encoder.finish()?)
}

fn rewrite_temp_fields<W: Write + Seek>(
    writer: &mut W,
    temp_offsets: Vec<u64>,
    data_offsets: Vec<u64>,
    compressed_sizes: Vec<u64>,
    checksums: Vec<(u32, u32)>,
) -> Result<()> {
    for (i, &position) in temp_offsets.iter().enumerate() {
        writer.seek(SeekFrom::Start(position))?;
        let offset = data_offsets[i];
        let size = compressed_sizes[i];
        let checksum = checksums[i];
        writer.write_all(&size.to_le_bytes())?;
        writer.write_all(&offset.to_le_bytes())?;
        writer.write_all(&checksum.0.to_le_bytes())?;
        writer.write_all(&checksum.1.to_le_bytes())?;
    }
    Ok(())
}
