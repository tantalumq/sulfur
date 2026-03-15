use std::{
    fmt::Display,
    io::{Read, Write},
};

use crate::{Error, HEADER_SIZE, MAX_FILE_COUNT, Result, SIGNATURE, VERSION};

pub struct Header {
    pub version: [u8; 2],
    pub file_count: Option<u32>,
    pub index_offset: Option<u64>,
}
#[allow(clippy::missing_errors_doc)]
impl Header {
    pub fn decode<R: Read>(reader: &mut R) -> Result<Self> {
        let mut buffer = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buffer)?;

        if &buffer[0..4] != SIGNATURE {
            return Err(Error::IncorrectSignature(
                "<Invalid SLF signature>".to_string(),
            ));
        }

        if buffer[4] != VERSION[0] {
            return Err(Error::UnsupportedVersion {
                expected: VERSION[0],
                found: buffer[4],
            });
        }

        let version = [buffer[4], buffer[5]];
        let file_count = u32::from_be_bytes(buffer[8..12].try_into()?);

        if file_count > MAX_FILE_COUNT {
            return Err(Error::TooManyFiles {
                expected: MAX_FILE_COUNT,
                found: file_count,
            });
        }

        let file_count = Some(file_count);

        let index_offset = Some(u64::from_be_bytes(buffer[16..24].try_into()?));

        Ok(Self {
            version,
            file_count,
            index_offset,
        })
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buffer = [0u8; HEADER_SIZE];

        buffer[0..4].copy_from_slice(SIGNATURE);
        buffer[4..6].copy_from_slice(&self.version);
        buffer[8..12].copy_from_slice(&self.file_count.unwrap_or_default().to_be_bytes());
        buffer[16..24].copy_from_slice(&self.index_offset.unwrap_or_default().to_be_bytes());

        writer.write_all(&buffer)?;
        writer.flush()?;
        Ok(())
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Archive Version {}.{}\nFile count: {}",
            self.version[0],
            self.version[1],
            self.file_count
                .map_or_else(|| "N/A".to_string(), |c| c.to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::{HEADER_SIZE, SIGNATURE, VERSION};

    #[test]
    fn test_header_decoding_with_bad_signature() {
        let bad_signature = b"TEST";
        let mut cursor = Cursor::new(bad_signature);

        let result = Header::decode(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_decoding_success() -> Result<()> {
        let mut data = [0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(SIGNATURE);
        data[4..6].copy_from_slice(&VERSION);

        let mut cursor = Cursor::new(data);
        let header = Header::decode(&mut cursor)?;

        assert_eq!(header.version, VERSION);
        Ok(())
    }

    #[test]
    fn test_header_decoding_with_too_many_files() {
        let mut data = [0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(SIGNATURE);
        data[4..6].copy_from_slice(&VERSION);
        data[8..12].copy_from_slice(&u32::to_be_bytes(MAX_FILE_COUNT + 1));

        let mut cursor = Cursor::new(data);
        let result = Header::decode(&mut cursor);

        assert!(result.is_err());
    }

    #[test]
    fn test_header_decoding_with_unsupported_version() {
        let mut data = [0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(SIGNATURE);
        data[4..6].copy_from_slice(&[0, 0]);

        let mut cursor = Cursor::new(data);
        let result = Header::decode(&mut cursor);

        assert!(result.is_err());
    }
}
