use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
};

use sulfur_archive::{ArchiveReader, ArchiveWriter, Error, archive::entry::Entry};
use tempfile::tempdir;

#[test]
fn test_pack_and_extract_single_file() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file = source_dir.join("test.txt");
    fs::write(test_file, b"hello world from test file")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract_all(&archive_path, &extraction_dir)?;

    let extracted_file = extraction_dir.join("test.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::read(extracted_file)?, b"hello world from test file");

    Ok(())
}

#[test]
fn test_pack_and_extract_empty_directory() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract_all(&archive_path, &extraction_dir)?;

    assert!(!extraction_dir.exists());

    Ok(())
}

#[test]
fn test_pack_and_extract_empty_file() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file = source_dir.join("test.txt");
    fs::write(test_file, b"")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract_all(&archive_path, &extraction_dir)?;

    let extracted_file = extraction_dir.join("test.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::read(extracted_file)?, b"");

    Ok(())
}

#[test]
fn test_pack_and_extract_multiple_files() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    for n in 0..10 {
        let test_file = source_dir.join(format!("test{n}.txt"));
        fs::write(test_file, b"hello world from test file")?;
    }

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract_all(&archive_path, &extraction_dir)?;

    for n in 0..10 {
        let extracted_file = extraction_dir.join(format!("test{n}.txt"));
        assert!(extracted_file.exists());
        assert_eq!(fs::read(extracted_file)?, b"hello world from test file");
    }

    Ok(())
}

#[test]
fn test_pack_and_extract_unicode_filename() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file = source_dir.join("😀.txt");
    fs::write(test_file, b"hello world from test file")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract_all(&archive_path, &extraction_dir)?;

    let extracted_file = extraction_dir.join("😀.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::read(extracted_file)?, b"hello world from test file");

    Ok(())
}

#[test]
fn test_extract_single_file_by_index() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file1 = source_dir.join("test1.txt");
    fs::write(test_file1, b"hello world from test file")?;
    let test_file2 = source_dir.join("test2.txt");
    fs::write(test_file2, b"hello world from test file")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    archive.extract(0, &extraction_dir)?;

    let extracted_file = extraction_dir.join("test1.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::read(extracted_file)?, b"hello world from test file");

    Ok(())
}

#[test]
fn test_extract_invalid_index() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let extraction_dir = dir.path().join("extraction");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file1 = source_dir.join("test1.txt");
    fs::write(test_file1, b"hello world from test file")?;
    let test_file2 = source_dir.join("test2.txt");
    fs::write(test_file2, b"hello world from test file")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;
    assert!(archive.extract(2, &extraction_dir).is_err());

    Ok(())
}

#[test]
fn test_archive_info_after_pack() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let source_dir = dir.path().join("source");
    let archive_path = dir.path().join("archive.slf");

    fs::create_dir(&source_dir)?;
    let test_file1 = source_dir.join("test1.txt");
    fs::write(test_file1, b"hello world from test file")?;
    let test_file2 = source_dir.join("test2.txt");
    fs::write(test_file2, b"hello world from test file")?;

    let file = File::create(&archive_path)?;
    let writer = BufWriter::new(file);
    let archive = ArchiveWriter::new(writer)?;
    archive.pack(&source_dir)?;

    let file = File::open(&archive_path)?;
    let reader = BufReader::new(file);
    let archive = ArchiveReader::open(reader)?;
    println!("{archive}");

    Ok(())
}

#[test]
fn test_extract_rejects_path_traversal() -> sulfur_archive::Result<()> {
    let dir = tempdir()?;

    let extraction_dir = dir.path().join("extraction");
    fs::create_dir(&extraction_dir)?;

    let entry = Entry {
        name: "../../path_traversal.txt".to_string(),
        source_size: 10,
        compressed_size: 10,
        source_checksum: 0,
        compressed_checksum: 0,
        offset: 0,
        data_start: 0,
    };

    let mut reader = std::io::Cursor::new(vec![0u8; 100]);
    let result = ArchiveReader::<()>::unpack_entry(&mut reader, &entry, 100, &extraction_dir);

    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(&err, Error::Path(msg) if msg.contains("path traversal")));

    let path = dir.path().join("path_traversal.txt");
    assert!(!path.exists());

    Ok(())
}
