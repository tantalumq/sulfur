use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use clap::{Parser, Subcommand};

use sulfur::{ArchiveReader, ArchiveWriter};

fn main() {
    let cli = Cli::parse();
    let result = run(cli);

    if let Err(e) = result {
        eprintln!("[ERROR] {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> sulfur::Result<()> {
    match cli.command {
        Command::Pack { source, output } => {
            let target = sulfur::archive_path(&source, &output)?;

            let file = File::create(target)?;
            let writer = BufWriter::new(file);
            let archive = ArchiveWriter::new(writer)?;
            archive.pack(&source)?;
            Ok(())
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
