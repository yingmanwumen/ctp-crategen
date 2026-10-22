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

    #[arg(short, long, value_name = "CRATE-NAME")]
    crate_name: Option<String>,

    /// Generate business error codes when the SDK contains error.xml.
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    error_xml: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => {
            if args.output_dir.exists() && !args.output_dir.is_dir() {
                anyhow::bail!("{} is not a directory", args.output_dir.display());
            }
            let crate_name = args.crate_name.unwrap_or(
                args.output_dir
                    .components()
                    .next_back()
                    .unwrap()
                    .as_os_str()
                    .display()
                    .to_string(),
            );

            let codegen = Codegen::new(args.output_dir, crate_name, args.error_xml);
            codegen.generate(&args.sdk_dir)?;
        }
    }
    Ok(())
}
