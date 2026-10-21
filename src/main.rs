use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use ctp_crategen::{codegen::Codegen, sdk::Sdk};

#[derive(Debug, Parser)]
#[command(about = "Generate CTP crates")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate crates from an SDK directory.
    Generate(GenerateArgs),
}

#[derive(Debug, Args)]
struct GenerateArgs {
    #[arg(value_name = "SDK-DIR", value_parser = Sdk::new)]
    sdk_dir: Sdk,

    #[arg(short, long, value_name = "OUTPUT-DIR")]
    output_dir: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => {
            let codegen = Codegen::new(args.output_dir);
            codegen.generate(&args.sdk_dir).unwrap();
        }
    }
}
