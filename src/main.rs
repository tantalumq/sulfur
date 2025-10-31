/*
TODO: File structure
TODO: TryInto Error
TODO: Use OsStr|OsString
TODO: Add windows support
TODO: Try macros for boiler-plate code
TODO: Versions
TODO: --help
TODO: User file unpack destination

.slf File structure:
Signature (4 bytes = '.slf'),
count of files (4 bytes),
length of file name(4 bytes),
name ('length' bytes),
original size of file (8 bytes),
compressed size (8 bytes),
data offset (8 bytes),
original checksum (4 bytes),
compressed checksum (4 bytes),
compressed file ('compressed size' bytes),
*/
pub mod error;
pub mod pack;
pub mod unpack;

use std::{
    env,
    ffi::OsString,
    io::{self, BufWriter, Read, Seek, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
};

use flate2::Crc;

use crate::error::{ArchiveError, Result};

pub const SIGNATURE: &[u8] = b".slf";
pub const BUFFER_SIZE: usize = 64 * 1024;

use pack::pack;
use unpack::unpack;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <pack|unpack> <directory|file>", args[0]);
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

pub struct HasherWriter<'a, W: Write> {
    writer: &'a mut W,
    hasher: Crc,
    bytes: u64,
}

impl<'a, W: Write + Seek> HasherWriter<'a, W> {
    pub fn new(writer: &'a mut W, hasher: Crc) -> Self {
        Self {
            writer,
            hasher,
            bytes: 0,
        }
    }

    pub fn sum(&self) -> u32 {
        self.hasher.sum()
    }

    pub fn stream_position(&mut self) -> error::Result<u64> {
        let pos = self.writer.stream_position()?;
        Ok(pos)
    }

    pub fn written_bytes(&mut self) -> u64 {
        let old = self.bytes;
        self.bytes = 0;
        old
    }
}
impl<'a, W: Write> Write for HasherWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.update(buf);
        let bytes = self.writer.write(buf)?;
        self.bytes += bytes as u64;
        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub struct InnerFile {
    name: OsString,
    original_size: u64,
    compressed_size: u64,
    offset: u64,
    original_checksum: u32,
    compressed_checksum: u32,
}

impl InnerFile {
    pub fn new(name: OsString) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn create(
        name: OsString,
        original_size: u64,
        compressed_size: u64,
        offset: u64,
        original_checksum: u32,
        compressed_checksum: u32,
    ) -> Self {
        let mut file = Self::new(name);
        file.set_original_size(original_size);
        file.set_compressed_size(compressed_size);
        file.set_offset(offset);
        file.set_original_checksum(original_checksum);
        file.set_compressed_checksum(compressed_checksum);
        file
    }

    pub fn from_archive<R: Read + Seek>(reader: &mut R, buffer: &mut [u8]) -> Result<Self> {
        let mut bytes = reader.read(&mut buffer[..4])?;

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

        bytes = reader.read(&mut buffer[..4])?;

        let original_checksum = if bytes == 4 {
            u32::from_le_bytes(buffer[..4].try_into().unwrap())
        } else {
            todo!("No original checksum")
        };

        bytes = reader.read(&mut buffer[..4])?;

        let compressed_checksum = if bytes == 4 {
            u32::from_le_bytes(buffer[..4].try_into().unwrap())
        } else {
            todo!("No compressed checksum")
        };

        Ok(InnerFile::create(
            name,
            original_size,
            compressed_size,
            offset,
            original_checksum,
            compressed_checksum,
        ))
    }

    pub fn write_metadata<W: Write + ?Sized + Seek>(
        &self,
        writer: &mut BufWriter<W>,
    ) -> Result<u64> {
        writer.write_all(&(self.name.len() as u32).to_le_bytes())?;
        writer.write_all(&self.name.as_bytes())?;
        writer.write_all(&self.original_size.to_le_bytes())?;
        let position = writer.stream_position()?;
        writer.write_all(&self.compressed_size.to_le_bytes())?;
        writer.write_all(&self.offset.to_le_bytes())?;
        writer.write_all(&self.original_checksum.to_le_bytes())?;
        writer.write_all(&self.compressed_checksum.to_le_bytes())?;
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
            name: OsString::default(),
            original_size: u64::default(),
            compressed_size: u64::default(),
            offset: u64::default(),
            original_checksum: u32::default(),
            compressed_checksum: u32::default(),
        }
    }
}

fn validate_path(path: &Path) -> Result<OsString> {
    let os_path = path.as_os_str().to_ascii_lowercase();

    if !path.exists() {
        return Err(ArchiveError::Path(format!(
            "File or directory doesn't exist at this path: {}",
            os_path.display()
        )));
    };
    Ok(os_path)
}
