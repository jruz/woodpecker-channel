mod commands;
mod core;
mod libs;
mod utils;

use std::process::ExitCode;

use clap::Parser as _;
use commands::{serve, version};
use utils::output::Output;

#[derive(Debug, clap::Parser)]
#[command(version = version::text(), about = "Woodpecker CI channel for Claude Code")]
struct Arguments {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, Debug, clap::Subcommand)]
enum Command {
    #[command(about = "Run the MCP channel server over stdio")]
    Serve,
    #[command(about = "Show version and build metadata")]
    Version,
}

fn run(command: Command) -> Output {
    match command {
        Command::Serve => serve::run(),
        Command::Version => version::run(),
    }
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let Some(command) = arguments.command else {
        return ExitCode::SUCCESS;
    };
    let output = run(command);
    if output.success {
        print!("{}", output.text);
        return ExitCode::SUCCESS;
    }
    eprint!("{}", output.text);
    ExitCode::FAILURE
}
