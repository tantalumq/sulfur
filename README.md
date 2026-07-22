# Sulfur
A fast and small file archiver written in Rust, using zstd compression

## Installation

```bash
cargo install slf-cli
```

Or as library:

```bash
cargo add sulfur-archive
```

## CLI usage

### Pack a directory or file

```bash
slf pack ./source -o ./archive.slf
```

### Unpack an archive

```bash
slf unpack ./archive.slf -o ./output/
```


### Extract single file by its internal path

```bash
slf get ./archive.slf src/main.rs -o ./output/
```

### Show info

```bash
slf info archive.slf
```

## Library usage

### Pack and unpack
```rust
use sulfur_archive::{ArchiveWriter, ArchiveReader};
use std::fs::File;
use std::io::{BufReader, BufWriter};

// Pack
let file = File::create("archive.slf")?;
let archive = ArchiveWriter::new(BufWriter::new(file))?;
let (writer, stats) = archive.pack("source/")?;

// Unpack
let file = File::open("archive.slf")?;
let mut archive = ArchiveReader::open(BufReader::new(file))?;
archive.extract_all("archive.slf", "output/")?;
```

### Extract single file by index
```rust
let file = File::open("archive.slf")?;
let mut archive = ArchiveReader::open(BufReader::new(file))?;
archive.extract(0, "output/")?;
```

### Print archive info
```rust
let file = File::open("archive.slf")?;
let archive = ArchiveReader::open(BufReader::new(file))?;

println!("{archive}")
```

### Streaming
```rust
use std::io::Read;

let file = File::open("archive.slf")?;
let archive = ArchiveReader::open(BufReader::new(file))?;

let entry = &archive.entries()[0];
let mut reader = entry.into_reader(file)?;

let mut content = Vec::new();
reader.read_to_end(&mut content)?;
```

## Format Structure (.slf)

```text
[Header - 24 bytes]
  - Signature (4 bytes = '.slf'),
  - Version (2 bytes = 'xx' ),
  - Padding (2 bytes),
  - File count (4 bytes),
  - Padding (4 bytes),
  - Index offset (8 bytes)

[Data blocks] (for each file)
  - Length of file name (4 bytes),
  - Name ('length' bytes),
  - Source size of file (8 bytes),
  - Source checksum (4 bytes),
  - Compressed size of file (8 bytes),
  - Compressed checksum (4 bytes),
  - Compressed file ('compressed size' bytes),
...
Index array (8 bytes * File count):
  - Data blocks offsets
```

## License

MIT OR Apache-2.0
