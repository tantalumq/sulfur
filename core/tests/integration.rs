use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
};

use sulfur::{ArchiveReader, ArchiveWriter};
use tempfile::tempdir;

#[test]
fn test_pack_and_extract_flow() -> sulfur::Result<()> {
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
    archive.extract_all(&extraction_dir)?;

    let extracted_file = extraction_dir.join("test.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::read(extracted_file)?, b"hello world from test file");

    Ok(())
}
