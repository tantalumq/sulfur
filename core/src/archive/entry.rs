use std::io::{Read, Seek, SeekFrom, Write};

use crate::{
    ENTRY_SIZE, Error, MAX_FILE_COMPRESSED_SIZE, MAX_FILE_SOURCE_SIZE, MAX_FILENAME_SIZE,
    NAME_LEN_SIZE, Result, SOURCE_SIZE_SIZE,
};

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub source_size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub source_checksum: Option<u32>,
    pub compressed_checksum: Option<u32>,
    pub offset: Option<u64>,
    pub data_start: Option<u64>,
}
#[allow(clippy::missing_errors_doc)]
impl Entry {
    pub fn decode<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut name_len_bytes = [0u8; 4];

        reader.read_exact(&mut name_len_bytes)?;
        let name_len_u32 = u32::from_be_bytes(name_len_bytes);
        let name_len: usize = name_len_u32.try_into()?;

        if name_len == 0 {
            return Err(Error::EmptyFilename);
        }

        if name_len_u32 > MAX_FILENAME_SIZE {
            return Err(Error::IncorrectEntry(format!(
                "file name length is greater than {MAX_FILENAME_SIZE}"
            )));
        }
        let mut name_bytes = vec![0u8; name_len];

        reader.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes)?;

        let mut metadata_bytes = [0u8; 24];

        reader.read_exact(&mut metadata_bytes)?;

        let source_size = u64::from_be_bytes(metadata_bytes[0..8].try_into()?);

        if source_size > MAX_FILE_SOURCE_SIZE {
            return Err(Error::IncorrectEntry(format!(
                "source file size is greater than {MAX_FILE_SOURCE_SIZE}"
            )));
        }

        let compressed_size = u64::from_be_bytes(metadata_bytes[12..20].try_into()?);

        if compressed_size > MAX_FILE_COMPRESSED_SIZE {
            return Err(Error::IncorrectEntry(format!(
                "compressed file size is greater than {MAX_FILE_COMPRESSED_SIZE}"
            )));
        }

        let data_start = Some(reader.stream_position()?);
        let source_size = Some(source_size);
        let source_checksum = Some(u32::from_be_bytes(metadata_bytes[8..12].try_into()?));
        let compressed_size = Some(compressed_size);
        let compressed_checksum = Some(u32::from_be_bytes(metadata_bytes[20..24].try_into()?));

        Ok(Self {
            name,
            source_size,
            source_checksum,
            compressed_size,
            compressed_checksum,
            offset: None,
            data_start,
        })
    }

    pub fn write<W: Write + Seek>(&mut self, mut writer: W) -> Result<()> {
        self.offset = Some(writer.stream_position()?);

        let name_len = self.name.len();
        let name_len_u32: u32 = self.name.len().try_into()?;

        if name_len == 0 {
            return Err(Error::EmptyFilename);
        }
        if name_len_u32 > MAX_FILENAME_SIZE {
            return Err(Error::IncorrectEntry(format!(
                "file name length is greater that {MAX_FILENAME_SIZE}",
            )));
        }

        let mut buffer = vec![0u8; ENTRY_SIZE + name_len];

        buffer[0..4].copy_from_slice(&name_len_u32.to_be_bytes());
        buffer[4..name_len + 4].copy_from_slice(self.name.as_bytes());
        buffer[name_len + 4..name_len + 12]
            .copy_from_slice(&self.source_size.unwrap_or_default().to_be_bytes());
        buffer[name_len + 12..name_len + 16]
            .copy_from_slice(&self.source_checksum.unwrap_or_default().to_be_bytes());
        buffer[name_len + 16..name_len + 24]
            .copy_from_slice(&self.compressed_size.unwrap_or_default().to_be_bytes());
        buffer[name_len + 24..name_len + 28]
            .copy_from_slice(&self.compressed_checksum.unwrap_or_default().to_be_bytes());

        writer.write_all(&buffer)?;

        self.data_start = Some(writer.stream_position()?);

        Ok(())
    }

    pub fn update<W: Write + Seek>(&self, mut writer: W) -> Result<()> {
        let name_len: u64 = self.name.len().try_into()?;

        let empty_fields_offset = self
            .offset
            .ok_or(Error::Empty(String::from("get offset from file entry")))?
            + NAME_LEN_SIZE
            + name_len
            + SOURCE_SIZE_SIZE;

        writer.seek(SeekFrom::Start(empty_fields_offset))?;

        let mut buffer = [0u8; 16];
        buffer[0..4].copy_from_slice(
            &self
                .source_checksum
                .ok_or(Error::Empty(String::from(
                    "get source checksum from file entry",
                )))?
                .to_be_bytes(),
        );
        buffer[4..12].copy_from_slice(
            &self
                .compressed_size
                .ok_or(Error::Empty(String::from(
                    "get compressed size from file entry",
                )))?
                .to_be_bytes(),
        );
        buffer[12..16].copy_from_slice(
            &self
                .compressed_checksum
                .ok_or(Error::Empty(String::from(
                    "get compressed checksum from file entry",
                )))?
                .to_be_bytes(),
        );
        writer.write_all(&buffer)?;

        writer.seek(SeekFrom::End(0))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_entry_decode_with_empty_name() {
        let data = [0u8; 4];
        let mut cursor = Cursor::new(data);

        let result = Entry::decode(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_entry_decode_with_buffer_overflow() {
        let data = u32::to_be_bytes(MAX_FILENAME_SIZE + 1);
        let mut cursor = Cursor::new(data);

        let result = Entry::decode(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_entry_decode_with_eof_name() {
        let mut data = vec![0u8; 4];
        data[..].copy_from_slice(&u32::to_be_bytes(16));
        data.extend_from_slice(b"too_short");
        let mut cursor = Cursor::new(data);

        let result = Entry::decode(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_entry_decode_with_invalid_utf8() {
        let mut data = vec![0u8; 4];
        data[..].copy_from_slice(&u32::to_be_bytes(4));
        data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        data.extend_from_slice(&[0u8; 24]);
        let mut cursor = Cursor::new(data);

        let result = Entry::decode(&mut cursor);
        assert!(result.is_err());
    }
    #[test]
    fn test_entry_decode_with_eof_metadata() {
        let mut data = vec![0u8; 4];
        data[..].copy_from_slice(&u32::to_be_bytes(4));
        data.extend_from_slice(b"name");
        data.extend_from_slice(&[0u8; 10]); // expected 24
        let mut cursor = Cursor::new(data);

        let result = Entry::decode(&mut cursor);
        assert!(result.is_err());
    }
}
