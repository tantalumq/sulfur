use std::{
    env,
    fmt,
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf, StripPrefixError},
};

use flate2::{Compression, write::GzEncoder, Crc};
use walkdir::WalkDir;

const SIGNATURE: &[u8] = b".slf";
const BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug)]
enum ArchiveError {
    Io(String),
    Path(String),
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(c) => write!(f, "[ERROR] {}", c),
            Self::Path(c) => write!(f, "[ERROR] {}", c),
        }
    }
}
impl From<io::Error> for ArchiveError {
    fn from(value: io::Error) -> Self {
        ArchiveError::Io(value.to_string())
    }
}
impl From<StripPrefixError> for ArchiveError {
    fn from(value: StripPrefixError) -> Self {
        ArchiveError::Path(value.to_string())
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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} pack <directory|file>", args[0]);
        return;
    }

    match args[1].as_str() {
        "pack" => {
            if let Err(e) = pack(Path::new(&args[2])) {
                eprintln!("{}", e)
            }
        }
        "unpack" => todo!(),
        _ => todo!(),
    }
}

fn pack(root: &Path) -> Result<()> {
    let archive_name = if root.is_file() {
        root.file_stem().ok_or(ArchiveError::Path(format!(
            "Failed to get file stem from path: {}",
            root.to_string_lossy()
        )))?
    } else {
        root.file_name().ok_or(ArchiveError::Path(format!(
            "Failed to get directory name from path: {}",
            root.to_string_lossy()
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

    let mut metadata = Vec::new();

    /*  .slf File structure:
        Signture (4 bytes = '.slf'),
        count of files (4 bytes),
        lenght of file name(4 bytes),
        name ('lenght' bytes),
        original size of file (8 bytes),
        compressed size (8 bytes),
        data offset (8 bytes),
        compressed file ('compressed size' bytes),
        original checksum (4 bytes),
        compressed checksum (4 bytes),
    */

    for path in &files {
        let relative_name = if root.is_file() {
            Path::new(path.file_name().ok_or(ArchiveError::Path(format!(
                "Failed to get file name from path: {}",
                path.to_string_lossy()
            )))?)
        } else {
            path.strip_prefix(root)?
        };
        let name_str = relative_name.to_string_lossy().to_string();
        let name_len = name_str.len() as u32;
        let file_size = path.metadata()?.len();

        metadata.push((name_len, name_str, file_size));
    }

    let mut temp_offsets = Vec::new();

    for (len, name, size) in metadata {
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(name.as_bytes())?;
        writer.write_all(&size.to_le_bytes())?;
        temp_offsets.push(writer.stream_position()?);
        writer.write_all(&[0u8; 8])?; // temp compressed size
        writer.write_all(&[0u8; 8])?; // temp offset
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

            let original_chunk = &buffer[0..bytes];

            original_checksum.update(&original_chunk);

            encoder.write_all(original_chunk)?;
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
