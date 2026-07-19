use std::{
    fmt::Display,
    io::{Read, Seek, SeekFrom, Write},
};

use crate::{Error, HEADER_SIZE, MAX_FILE_COUNT, Result, SIGNATURE, VERSION};

pub struct Header {
    pub version: [u8; 2],
    pub file_count: u32,
    pub index_offset: u64,
}
#[allow(clippy::missing_errors_doc)]
impl Header {
    pub fn decode<R: Read + Seek>(reader: &mut R) -> Result<Self> {
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

        let index_offset = u64::from_be_bytes(buffer[16..24].try_into()?);

        let position = reader.stream_position()?;
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(position))?;

        if index_offset > file_size {
            return Err(Error::IncorrectIndexOffset(String::from(
                "index offset is greater than file size",
            )));
        }

        if index_offset < HEADER_SIZE as u64 {
            return Err(Error::IncorrectIndexOffset(String::from(
                "index offset is less than header size",
            )));
        }

        let file_count_bytes =
            file_count
                .checked_mul(8)
                .ok_or(Error::IncorrectFileCount(String::from(
                    "the index array doesn't fits in file because of too big count of files",
                )))?;

        if index_offset
            .checked_add(u64::from(file_count_bytes))
            .is_none()
        {
            return Err(Error::IncorrectIndexOffset(String::from(
                "the index array doesn't fits in file becaues of too big value of index offset",
            )));
        }

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
        buffer[8..12].copy_from_slice(&self.file_count.to_be_bytes());
        buffer[16..24].copy_from_slice(&self.index_offset.to_be_bytes());

        writer.write_all(&buffer)?;
        Ok(())
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Archive Version {}.{}\nFile count: {}",
            self.version[0], self.version[1], self.file_count
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
        data[16..24].copy_from_slice(&u64::to_be_bytes(HEADER_SIZE.try_into()?));

        let mut cursor = Cursor::new(data);

        assert!(Header::decode(&mut cursor).is_ok());

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

    #[test]
    fn test_header_decoding_with_incorrect_index_offset() {
        let mut data = [0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(SIGNATURE);
        data[4..6].copy_from_slice(&VERSION);
        data[16..24].copy_from_slice(&u64::MAX.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = Header::decode(&mut cursor);

        assert!(result.is_err());
    }
}
