use clap::{Args, Parser, Subcommand};
use ctp_crategen::sdk_dir::SdkDir;

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
    #[arg(value_name = "SDK-DIR", value_parser = SdkDir::new)]
    sdk_dir: SdkDir,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(_args) => {
            // SDK validation is performed by the clap value parser before this branch.
            // The actual crate generation pipeline will be wired in here.
            todo!()
        }
    }
}
