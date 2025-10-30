// TODO: File structure
// TODO: TryInto Error
// TODO: Use OsStr|OsString
// TODO: Add windows support

// .slf File structure:
// Signature (4 bytes = '.slf'),
// count of files (4 bytes),
// length of file name(4 bytes),
// name ('length' bytes),
// original size of file (8 bytes),
// compressed size (8 bytes),
// data offset (8 bytes),
// compressed file ('compressed size' bytes),
// original checksum (4 bytes),
// compressed checksum (4 bytes),

use std::{
    env,
    ffi::OsString,
    fmt,
    fs::{File, create_dir},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf, StripPrefixError},
};

use flate2::{
    Compression, Crc,
    write::{GzDecoder, GzEncoder},
};
use walkdir::WalkDir;

const SIGNATURE: &[u8] = b".slf";
const BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug)]
enum ArchiveError {
    Io(String),
    Path(String),
    Cast(String),
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(c) => write!(f, "{}", c),
            Self::Path(c) => write!(f, "{}", c),
            Self::Cast(c) => write!(f, "{}", c),
        }
    }
}
impl From<io::Error> for ArchiveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
impl From<StripPrefixError> for ArchiveError {
    fn from(value: StripPrefixError) -> Self {
        Self::Path(value.to_string())
    }
}

type Result<T> = std::result::Result<T, ArchiveError>;

struct HasherWriter<W: Write> {
    writer: W,
    hasher: Crc,
}

impl<W: Write> HasherWriter<W> {
    fn new(writer: W, hasher: Crc) -> Self {
        Self { writer, hasher }
    }

    fn sum(self) -> u32 {
        self.hasher.sum()
    }
}
impl<W: Write> Write for HasherWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.update(buf);
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

struct InnerFile {
    name: String,
    original_size: u64,
    compressed_size: u64,
    offset: u64,
    original_checksum: u32,
    compressed_checksum: u32,
}

impl InnerFile {
    fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    fn create(name: String, original_size: u64, compressed_size: u64, offset: u64) -> Self {
        Self {
            name,
            original_size,
            compressed_size,
            offset,
            ..Default::default()
        }
    }

    fn write_metadata<W: Write + ?Sized + Seek>(&self, writer: &mut BufWriter<W>) -> Result<u64> {
        writer.write_all(&(self.name.len() as u32).to_le_bytes())?;
        writer.write_all(&self.name.as_bytes())?;
        writer.write_all(&self.original_size.to_le_bytes())?;
        let position = writer.stream_position()?;
        writer.write_all(&self.compressed_size.to_le_bytes())?;
        writer.write_all(&self.offset.to_le_bytes())?;
        Ok(position)
    }

    fn set_original_size(&mut self, size: u64) {
        self.original_size = size
    }

    fn set_compressed_size(&mut self, size: u64) {
        self.compressed_size = size
    }

    fn set_offset(&mut self, offset: u64) {
        self.offset = offset
    }

    fn set_original_checksum(&mut self, checksum: u32) {
        self.original_checksum = checksum
    }

    fn set_compressed_checksum(&mut self, checksum: u32) {
        self.compressed_checksum = checksum
    }
}

impl Default for InnerFile {
    fn default() -> Self {
        Self {
            name: String::default(),
            original_size: u64::default(),
            compressed_size: u64::default(),
            offset: u64::default(),
            original_checksum: u32::default(),
            compressed_checksum: u32::default(),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} pack <directory|file>", args[0]);
        return;
    }

    let result = match args[1].as_str() {
        "pack" => pack(Path::new(&args[2])),
        "unpack" => unpack(&Path::new(&args[2])),
        _ => todo!(),
    };

    if let Err(e) = result {
        eprintln!("[ERROR] {}", e)
    }
}

fn pack(root: &Path) -> Result<()> {
    let str_path = root
        .to_str()
        .ok_or(ArchiveError::Path(format!("Can't read path")))?;

    if !root.exists() {
        return Err(ArchiveError::Path(format!(
            "File or directory doesn't exist at this path: {}",
            str_path
        )));
    }

    let archive_name = if root.is_file() {
        root.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            str_path
        )))?
    } else {
        root.file_name().ok_or(ArchiveError::Path(format!(
            "Failed to get directory name from path: {}",
            str_path
        )))?
    };

    let archive_path = PathBuf::from(archive_name).with_extension("slf");
    let file = File::create(archive_path)?;
    let mut writer = BufWriter::new(file);

    writer.write_all(SIGNATURE)?;

    let files: Vec<PathBuf> = if root.is_file() {
        vec![root.to_path_buf()]
    } else {
        WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect()
    };

    writer.write_all(&(files.len() as u32).to_le_bytes())?;

    let mut inners = Vec::new();

    for path in &files {
        let relative_name = if root.is_file() {
            Path::new(path.file_name().ok_or(ArchiveError::Path(format!(
                "Failed to get file name from path: {}",
                str_path
            )))?)
        } else {
            path.strip_prefix(root)?
        };
        let file_name = relative_name.to_string_lossy().to_string();
        let file_size = path.metadata()?.len();

        let inner_file = InnerFile::create(file_name, file_size, 0, 0);

        inners.push(inner_file);
    }

    let mut temp_offsets = Vec::new();

    for inner in inners {
        temp_offsets.push(inner.write_metadata(&mut writer)?);
    }

    let mut data_offsets = Vec::new();

    for path in files {
        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; BUFFER_SIZE];

        let mut original_checksum = Crc::new();

        let hasher = Crc::new();
        let mut hasher_writer = HasherWriter::new(&mut writer, hasher);

        data_offsets.push(hasher_writer.writer.stream_position()?);

        let mut encoder = GzEncoder::new(&mut hasher_writer, Compression::default());

        loop {
            let bytes = reader.read(&mut buffer)?;

            if bytes == 0 {
                break; //EOF
            }

            let chunk = &buffer[..bytes];

            original_checksum.update(&chunk);

            encoder.write_all(chunk)?;
        }

        encoder.finish()?;

        let original_checksum = original_checksum.sum();
        let compressed_checksum = hasher_writer.sum();

        writer.write_all(&original_checksum.to_le_bytes())?;
        writer.write_all(&compressed_checksum.to_le_bytes())?;

        writer.flush()?;
    }

    for (i, &position) in temp_offsets.iter().enumerate() {
        writer.seek(SeekFrom::Start(position))?;
        let offset = data_offsets[i];
        writer.write_all(&(offset - position).to_le_bytes())?;
        writer.write_all(&data_offsets[i].to_le_bytes())?;
    }

    writer.seek(SeekFrom::End(0))?;

    writer.flush()?;
    Ok(())
}

fn unpack(archive: &Path) -> Result<()> {
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

    let mut bytes = reader.read(&mut buffer[..4])?;

    if bytes != 4 && buffer != SIGNATURE {
        return Err(ArchiveError::Path(format!(
            "File is corrupted or have incorrect type. File at path: {}",
            str_path
        )));
    }

    bytes = reader.read(&mut buffer)?;

    let file_count = if bytes == 4 {
        u32::from_le_bytes(buffer[..4].try_into().unwrap())
    } else {
        todo!("No files")
    };

    let dir_pathbuf = archive.with_extension("");
    let dir_path = dir_pathbuf.as_path();

    if file_count > 1 {
        create_dir(dir_path)?;
    }

    for i in 0..file_count {
        bytes = reader.read(&mut buffer[..4])?;

        let name_len = if bytes == 4 {
            u32::from_le_bytes(buffer[..4].try_into().unwrap())
        } else {
            todo!("No file name")
        };

        bytes = reader.read(&mut buffer[..(name_len as usize)])?;

        let name = if bytes == name_len as usize {
            OsString::from_vec(buffer[..bytes].to_vec())
        } else {
            todo!("Empty file name")
        };

        bytes = reader.read(&mut buffer[..8])?;

        let original_size = if bytes == 8 {
            u64::from_le_bytes(buffer[..8].try_into().unwrap())
        } else {
            todo!("Empty files")
        };

        bytes = reader.read(&mut buffer[..8])?;

        let compressed_size = if bytes == 8 {
            u64::from_le_bytes(buffer[..8].try_into().unwrap())
        } else {
            todo!("Empty files")
        };

        bytes = reader.read(&mut buffer[..8])?;

        let offset = if bytes == 8 {
            u64::from_le_bytes(buffer[..8].try_into().unwrap())
        } else {
            todo!("No offset")
        };

        let file_path = dir_path.join(name);
        let file = File::create(file_path)?;

        let mut writer = BufWriter::new(file);

        let position = reader.stream_position()?;
        reader.seek(SeekFrom::Start(offset))?;

        let mut compressed_checksum = Crc::new();

        let hasher = Crc::new();
        let mut hasher_writer = HasherWriter::new(&mut writer, hasher);

        let mut decoder = GzDecoder::new(&mut hasher_writer);

        let mut remaining_bytes = compressed_size;

        loop {
            let to_read = remaining_bytes.min(BUFFER_SIZE as u64) as usize;

            reader.read(&mut buffer[..to_read])?;

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

        bytes = reader.read(&mut buffer[..4])?;

        let original_checksum_readed = if bytes == 4 {
            u32::from_le_bytes(buffer[..4].try_into().unwrap())
        } else {
            todo!("No file name")
        };

        bytes = reader.read(&mut buffer[..4])?;

        let compressed_checksum_readed = if bytes == 4 {
            u32::from_le_bytes(buffer[..4].try_into().unwrap())
        } else {
            todo!("No file name")
        };

        if original_checksum != original_checksum_readed {
            todo!("original cheksums aren't equal")
        }

        if compressed_checksum != compressed_checksum_readed {
            todo!("compressed cheksums aren't equal")
        }
    }

    Ok(())
}
