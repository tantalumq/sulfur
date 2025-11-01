/*
.slf File structure:
Signature (4 bytes = '.slf'),
version (2 bytes),
count of files (4 bytes),
index offset (8 bytes)
 | length of file name(4 bytes),
 | name ('length' bytes),
 | original size of file (8 bytes),
 | compressed size (8 bytes),
 | original checksum (4 bytes),
 | compressed checksum (4 bytes),
 | compressed file ('compressed size' bytes),
 ...
Index array (8 bytes * File count).
*/

pub mod error;
pub mod pack;
pub mod unpack;

use std::{
    env,
    ffi::OsString,
    fs::File,
    io::{self, BufWriter, Read, Seek, Write},
    path::{Component, PathBuf},
};

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;

use flate2::Crc;

use crate::error::{ArchiveError, Result};

pub const SIGNATURE: &[u8] = b".slf";
pub const VERSION: [u8; 2] = [1, 0]; // 1.0
pub const BUFFER_SIZE: usize = 128 * 1024;

use pack::pack;
use unpack::unpack;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <pack|unpack> <directory|file>", args[0]);
        return;
    }

    let target = if let Some(target) = args.get(3) {
        Some(PathBuf::from(target))
    } else {
        None
    };

    let result = match args[1].as_str() {
        "pack" => pack(PathBuf::from(&args[2]), target),
        "unpack" => unpack(PathBuf::from(&args[2]), target),
        _ => Err(ArchiveError::Io(format!(
            "Incorrect usage of '{}', see `--help` for more info",
            &args[1]
        ))),
    };

    if let Err(e) = result {
        eprintln!("[ERROR] {}", e)
    }
}

pub struct HasherWriter<'a> {
    writer: &'a mut BufWriter<File>,
    hasher: Crc,
    bytes: u64,
}

impl<'a> HasherWriter<'a> {
    pub fn new(writer: &'a mut BufWriter<File>, hasher: Crc) -> Self {
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

    pub fn take_written_bytes(&mut self) -> u64 {
        let old = self.bytes;
        self.bytes = 0;
        old
    }
}
impl<'a> Write for HasherWriter<'a> {
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
    original_checksum: u32,
    compressed_checksum: u32,
    position: u64,
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
        original_checksum: u32,
        compressed_checksum: u32,
    ) -> Self {
        let mut file = Self::new(name);
        file.set_original_size(original_size);
        file.set_compressed_size(compressed_size);
        file.set_original_checksum(original_checksum);
        file.set_compressed_checksum(compressed_checksum);
        file
    }

    pub fn from_archive<R: Read + Seek>(reader: &mut R, buffer: &mut [u8]) -> Result<Self> {
        reader.read_exact(&mut buffer[..4])?;
        let name_len = u32::from_le_bytes(buffer[..4].try_into()?) as usize;

        if name_len == 0 {
            return Err(ArchiveError::EmptyFilename);
        }

        if name_len as usize > BUFFER_SIZE {
            return Err(ArchiveError::BufferOverflow(name_len as usize));
        }

        reader.read_exact(&mut buffer[..(name_len as usize)])?;
        let name = OsString::from_vec(buffer[..(name_len as usize)].to_vec());

        reader.read_exact(&mut buffer[..8])?;
        let original_size = u64::from_le_bytes(buffer[..8].try_into()?);

        reader.read_exact(&mut buffer[..8])?;
        let compressed_size = u64::from_le_bytes(buffer[..8].try_into()?);

        reader.read_exact(&mut buffer[..4])?;
        let original_checksum = u32::from_le_bytes(buffer[..4].try_into()?);

        reader.read_exact(&mut buffer[..4])?;
        let compressed_checksum = u32::from_le_bytes(buffer[..4].try_into()?);

        Ok(InnerFile::create(
            name,
            original_size,
            compressed_size,
            original_checksum,
            compressed_checksum,
        ))
    }

    pub fn write_metadata<W: Write + ?Sized + Seek>(
        &mut self,
        writer: &mut BufWriter<W>,
    ) -> Result<u64> {
        self.position = writer.stream_position()?;
        let name_bytes = self.name.as_encoded_bytes();
        writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
        writer.write_all(name_bytes)?;
        writer.write_all(&self.original_size.to_le_bytes())?;
        let position = writer.stream_position()?;
        writer.write_all(&self.compressed_size.to_le_bytes())?;
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
            original_checksum: u32::default(),
            compressed_checksum: u32::default(),
            position: u64::default(),
        }
    }
}

fn normalize_path(path: &PathBuf) -> PathBuf {
    let mut normalized = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => {
                normalized.clear();
                normalized.push(Component::Prefix(p));
            }
            Component::RootDir => {
                normalized.clear();
                normalized.push(Component::RootDir);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = normalized.last() {
                    match last {
                        Component::Normal(_) => {
                            normalized.pop();
                        }
                        Component::RootDir => {}
                        _ => {}
                    }
                } else {
                    normalized.push(component);
                }
            }
            Component::Normal(_) => normalized.push(component),
        }
    }
    normalized.iter().collect()
}
