/*
.slf File structure:
Signature (4 bytes = '.slf'),
version (2 bytes = 'xx' ),
padding (2 bytes),
file count (4 bytes),
padding (4 bytes),
index offset (8 bytes)
 | length of file name (4 bytes),
 | name ('length' bytes),
 | source size of file (8 bytes),
 | source checksum (4 bytes),
 | compressed size of file (8 bytes),
 | compressed checksum (4 bytes),
 | compressed file ('compressed size' bytes),
 ...
Index array (8 bytes * File count).
*/

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use clap::{Parser, Subcommand, ValueEnum};

use sulfur::{ArchiveReader, ArchiveWriter, CompressionType as CoreCompressionType};

fn main() {
    let cli = Cli::parse();
    let result = run(cli);

    if let Err(e) = result {
        eprintln!("[ERROR] {e}");
        std::process::exit(1);
    }
}

#[allow(unused_variables)]
fn run(cli: Cli) -> sulfur::error::Result<()> {
    match cli.command {
        Command::Pack {
            source,
            output,
            compression_type,
        } => {
            let target = sulfur::archive_path(&source, &output)?;

            let file = File::create(target)?;
            let writer = BufWriter::new(file);
            let mut archive = ArchiveWriter::new(writer)?;
            archive.pack(&source)
        }
        Command::Unpack { source, output } => {
            let target = sulfur::extraction_path(&source, &output)?;

            let file = File::open(source)?;
            let reader = BufReader::new(file);
            let mut archive = ArchiveReader::open(reader)?;
            archive.extract_all(&target)
        }
        Command::Get {
            index,
            source,
            output,
        } => {
            let target = sulfur::extraction_path(&source, &output)?;

            let file = File::open(source)?;
            let reader = BufReader::new(file);
            let mut archive = ArchiveReader::open(reader)?;
            archive.extract(index, &target)
        }
        Command::Info { source } => {
            let file = File::open(source)?;
            let reader = BufReader::new(file);
            let archive = ArchiveReader::open(reader)?;
            println!("{archive}");
            Ok(())
        }
    }
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Pack {
        source: PathBuf,
        #[arg(short, long, default_value = "./")]
        output: PathBuf,
        #[arg(short = 'c', long = "compression", value_enum, default_value_t = CliCompressionType::Smart)]
        compression_type: CliCompressionType,
    },

    Unpack {
        source: PathBuf,
        #[arg(short = 'o', long, default_value = "./")]
        output: PathBuf,
    },
    Get {
        source: PathBuf,
        index: u32,
        #[arg(short = 'o', long, default_value = "./")]
        output: PathBuf,
    },
    Info {
        source: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CliCompressionType {
    Smart,
    Force,
    None,
}

impl From<CoreCompressionType> for CliCompressionType {
    fn from(value: CoreCompressionType) -> Self {
        match value {
            CoreCompressionType::Smart => CliCompressionType::Smart,
            CoreCompressionType::Force => CliCompressionType::Force,
            CoreCompressionType::None => CliCompressionType::None,
        }
    }
}

impl From<CliCompressionType> for CoreCompressionType {
    fn from(value: CliCompressionType) -> Self {
        match value {
            CliCompressionType::Smart => CoreCompressionType::Smart,
            CliCompressionType::Force => CoreCompressionType::Force,
            CliCompressionType::None => CoreCompressionType::None,
        }
    }
}
