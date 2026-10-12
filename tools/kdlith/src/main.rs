//! The kdlith CLI entry point.

use std::process::ExitCode;

const USAGE: &str = "\
kdlith - typed, streaming KDL processing and Rust processor generation

Usage: kdlith [--help | --version]

Options:
  -h, --help     Print this help and exit
  -V, --version  Print the version and exit

Commands are not implemented yet; any other invocation fails.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        [flag] if flag == "--version" || flag == "-V" => {
            println!("kdlith {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        // The verb dispatch body is not wired yet; until then every other
        // invocation fails closed rather than pretending to work.
        _ => {
            eprintln!("kdlith: not implemented yet");
            ExitCode::from(1)
        }
    }
}
