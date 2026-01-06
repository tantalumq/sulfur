/*
.slf File structure:
Signature (4 bytes = '.slf'),
version (2 bytes = 'xx' ),
file count (4 bytes),
index offset (8 bytes)
 | length of file name(4 bytes),
 | name ('length' bytes),
 | original size of file (8 bytes),
 | compressed size (8 bytes),
 | original checksum (4 bytes),
 | compressed checksum (4 bytes),
 | compressed file ('compressed size' bytes),
 ...
Index array (8 bytes * File count).
*/

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use sulfur_core::{get, pack, unpack};

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Pack { source, target } => pack(&source, &target),
        Command::Unpack { source, target } => unpack(&source, &target),
        Command::Get {
            source,
            target,
            index,
        } => get(&source, &target, index),
    };

    if let Err(e) = result {
        eprintln!("[ERROR] {e}");
        std::process::exit(1);
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
        #[arg(short = 'o', long, default_value = "./")]
        target: PathBuf,
    },

    Unpack {
        source: PathBuf,
        #[arg(short = 'o', long, default_value = "./")]
        target: PathBuf,
    },
    Get {
        source: PathBuf,
        #[arg(short = 'o', long, default_value = "./")]
        target: PathBuf,
        index: u32,
    },
}
