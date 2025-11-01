use std::{
    fs::{File, create_dir_all},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use flate2::{Compression, Crc, write::GzEncoder};
use walkdir::WalkDir;

use crate::{
    VERSION,
    error::{ArchiveError, Result},
    normalize_path,
};

use crate::{BUFFER_SIZE, HasherWriter, InnerFile, SIGNATURE};

pub fn pack(source: PathBuf, target: Option<PathBuf>) -> Result<()> {
    let target = if let Some(path) = target {
        path
    } else {
        PathBuf::from(source.parent().unwrap_or(Path::new(".")))
    };

    let archive_path = get_archive_path(&source, &target)?;
    if let Some(parents) = archive_path.parent() {
        create_dir_all(parents)?;
    }

    let file = File::create(archive_path)?;
    let mut writer = BufWriter::new(file);

    let files: Vec<PathBuf> = collect_files(&source);

    writer.write_all(SIGNATURE)?;
    writer.write_all(&VERSION)?;
    writer.write_all(&u32::try_from(files.len())?.to_le_bytes())?; //file count
    writer.write_all(&u64::to_le_bytes(0))?; //index offset

    let mut inners = inner_files(&source, &files)?;

    let (temp_offsets, compressed_sizes, checksums) =
        process_files(&mut inners, files, &mut writer)?;

    writer.flush()?;

    rewrite_temp_fields(&mut writer, temp_offsets, compressed_sizes, checksums)?;

    write_index_array(&mut writer, &inners)?;

    writer.flush()?;
    Ok(())
}

fn get_archive_path(source: &PathBuf, target: &PathBuf) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || (!source.is_file() && !source.is_dir()) {
        return Err(ArchiveError::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }
    Ok(if target.extension().map_or(false, |ex| ex == "slf") {
        target
    } else {
        let archive_name = get_archive_name(&source)?;
        target.join(archive_name).with_extension("slf")
    })
}

fn get_archive_name(source: &PathBuf) -> Result<PathBuf> {
    Ok(if source.is_file() {
        PathBuf::from(source.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?)
    } else {
        PathBuf::from(source.file_name().ok_or(ArchiveError::Path(format!(
            "Failed to get directory name from path: {}",
            source.display()
        )))?)
    })
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

        let inner_file = InnerFile::create(relative_name, file_size, 0, 0, 0);

        inners.push(inner_file);
    }
    Ok(inners)
}

fn process_files(
    inners: &mut Vec<InnerFile>,
    paths: Vec<PathBuf>,
    writer: &mut BufWriter<File>,
) -> Result<(Vec<u64>, Vec<u64>, Vec<(u32, u32)>)> {
    let mut temp_offsets = Vec::new();
    let mut compressed_sizes = Vec::new();
    let mut checksums = Vec::new();

    for (i, path) in paths.iter().enumerate() {
        let offset = inners[i].write_metadata(writer)?;

        let hasher = Crc::new();
        let hasher_writer = HasherWriter::new(writer, hasher);

        let (size, (original_cheksum, compressed_checksum)) =
            process_single_file(path, hasher_writer)?;

        temp_offsets.push(offset);
        compressed_sizes.push(size);
        checksums.push((original_cheksum, compressed_checksum));
    }

    Ok((temp_offsets, compressed_sizes, checksums))
}

fn process_single_file(
    path: &PathBuf,
    mut hasher_writer: HasherWriter,
) -> Result<(u64, (u32, u32))> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut buffer = [0u8; BUFFER_SIZE];

    let mut original_checksum = Crc::new();

    let encoder = GzEncoder::new(hasher_writer, Compression::default());

    hasher_writer =
        compress_file_content(&mut reader, encoder, &mut original_checksum, &mut buffer)?;

    let size = hasher_writer.take_written_bytes();

    let original_checksum = original_checksum.sum();
    let compressed_checksum = hasher_writer.sum();

    Ok((size, (original_checksum, compressed_checksum)))
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

fn rewrite_temp_fields(
    writer: &mut BufWriter<File>,
    temp_offsets: Vec<u64>,
    compressed_sizes: Vec<u64>,
    checksums: Vec<(u32, u32)>,
) -> Result<()> {
    let end = writer.stream_position()?;
    writer.seek(SeekFrom::Start(10))?;
    writer.write_all(&end.to_le_bytes())?;
    writer.flush()?;
    for (i, &position) in temp_offsets.iter().enumerate() {
        writer.seek(SeekFrom::Start(position))?;
        let size = compressed_sizes[i];
        let checksum = checksums[i];
        writer.write_all(&size.to_le_bytes())?;
        writer.write_all(&checksum.0.to_le_bytes())?;
        writer.write_all(&checksum.1.to_le_bytes())?;
        writer.flush()?;
    }
    writer.seek(SeekFrom::Start(end))?;
    Ok(())
}

fn write_index_array(writer: &mut BufWriter<File>, inners: &Vec<InnerFile>) -> Result<()> {
    for inner in inners {
        let position = inner.position;
        writer.write_all(&position.to_le_bytes())?;
    }

    Ok(())
}
