use clap::{Args, Parser, Subcommand};
use ctp_crategen::sdk::Sdk;

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
    /// SDK directory to process.
    #[arg(value_name = "SDK-DIR", value_parser = Sdk::new)]
    sdk_dir: Sdk,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => {
            println!("{:?}", args.sdk_dir);
        }
    }
}
