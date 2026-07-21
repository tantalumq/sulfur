use std::io::{self, BufReader, Read, Take};

use crc32fast::Hasher;

pub struct EntryReader<R: Read> {
    pub(crate) decoder: zstd::Decoder<'static, BufReader<Take<R>>>,
    pub(crate) hasher: Hasher,
    pub(crate) expected_checksum: u32,
    pub(crate) expected_size: u64,
    pub(crate) bytes_read: u64,
}

impl<R: Read> Read for EntryReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_read = self.decoder.read(buf)?;

        if bytes_read == 0 {
            if self.bytes_read != self.expected_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "size mismatch: expected {}, got {}",
                        self.expected_size, self.bytes_read
                    ),
                ));
            }

            if self.hasher.clone().finalize() != self.expected_checksum {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "checksum mismatch",
                ));
            }
        } else {
            self.bytes_read += u64::try_from(bytes_read).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "can't change type from usize to u64",
                )
            })?;

            if self.bytes_read > self.expected_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "decompressed size is bigger than expected",
                ));
            }

            self.hasher.update(&buf[..bytes_read]);
        }

        Ok(bytes_read)
    }
}
