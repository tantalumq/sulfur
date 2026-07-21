use std::io::{self, Write};

use crc32fast::Hasher;

use crate::archive::HasherWriter;

#[allow(clippy::missing_errors_doc)]
impl<W: Write> HasherWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: Hasher::new(),
            bytes: 0,
        }
    }

    pub fn sum(&self) -> u32 {
        self.hasher.clone().finalize()
    }

    pub fn take_and_reset_bytes(&mut self) -> u64 {
        let old = self.bytes;
        self.bytes = 0;
        old
    }
}

impl<W: Write> Write for HasherWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let bytes = self.writer.write(buf)?;
        self.hasher.update(&buf[..bytes]);
        self.bytes += bytes as u64;
        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
