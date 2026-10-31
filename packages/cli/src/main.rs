//! `confy` binary entry point; all logic lives in the [`confy_cli`] library.

use clap::Parser as _;

fn main() -> std::process::ExitCode {
    confy_cli::run(confy_cli::Cli::parse()).unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        std::process::ExitCode::from(2)
    })
}
