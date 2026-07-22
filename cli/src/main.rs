use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    time::Instant,
};

use clap::{Parser, Subcommand};

use sulfur_archive::{ArchiveReader, ArchiveWriter, Error, to_readable_bytes};

fn main() {
    let cli = Cli::parse();
    let result = run(cli);

    if let Err(e) = result {
        eprintln!("[ERROR] {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> sulfur_archive::Result<()> {
    let start = Instant::now();

    match cli.command {
        Command::Pack {
            source,
            output,
            compression,
        } => {
            let target = sulfur_archive::archive_path(&source, &output)?;

            if let Some(parents) = target.parent() {
                std::fs::create_dir_all(parents)?;
            }

            let file = File::create(target)?;
            let writer = BufWriter::new(file);
            let archive = ArchiveWriter::new(writer)?.with_compression_level(compression);
            let (_, stats) = archive.pack(&source)?;

            let ratio = if stats.source_size != 0 {
                format!(
                    "{}%",
                    100u64.saturating_sub(
                        (stats.compressed_size * 100)
                            .checked_div(stats.source_size)
                            .expect("Can't divide by source size")
                    )
                )
            } else {
                "N/A".to_string()
            };

            println!("Successfully packed {} files", stats.file_count);
            println!("Source size:      {}", to_readable_bytes(stats.source_size));
            println!(
                "Compressed size:  {}",
                to_readable_bytes(stats.compressed_size)
            );
            println!("Saved:            {ratio}");
            println!("Time:             {:.2?}", start.elapsed());
            Ok(())
        }
        Command::Unpack { source, output } => {
            let target = sulfur_archive::extraction_path(&source, &output)?;

            let file = File::open(&source)?;
            let reader = BufReader::new(file);
            let mut archive = ArchiveReader::open(reader)?;
            archive.extract_all(&source, &target)?;
            println!("Successfully extracted in {:.2?}", start.elapsed());
            Ok(())
        }
        Command::Get {
            name,
            source,
            output,
        } => {
            let target = sulfur_archive::extraction_path(&source, &output)?;

            let file = File::open(source)?;
            let reader = BufReader::new(file);
            let mut archive = ArchiveReader::open(reader)?;

            let archive_map = archive.get_entries_map()?;
            let index = archive_map
                .get(name.as_str())
                .copied()
                .ok_or(Error::IncorrectFileName(format!(
                    "can't find {name} in archive"
                )))?;

            archive.extract(index, &target)?;
            println!("Successfully got file in {:.2?}", start.elapsed());
            Ok(())
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
        #[arg(short, long, default_value = "3")]
        compression: i32,
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
