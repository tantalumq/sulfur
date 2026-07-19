use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use clap::{Parser, Subcommand};

use sulfur::{ArchiveReader, ArchiveWriter, Error};

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

            if let Some(parents) = target.parent() {
                std::fs::create_dir_all(parents)?;
            }

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
            name,
            source,
            output,
        } => {
            let target = sulfur::extraction_path(&source, &output)?;

            let file = File::open(source)?;
            let reader = BufReader::new(file);
            let mut archive = ArchiveReader::open(reader)?;

            let archive_map = archive.get_entries_map()?;
            let index = archive_map
                .get(name.as_str())
                .copied()
                .ok_or(Error::IncorrectFileName(format!(
                    "can't found {name} in archive"
                )))?;

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
        name: String,
        #[arg(short = 'o', long, default_value = "./")]
        output: PathBuf,
    },
    Info {
        source: PathBuf,
    },
}
