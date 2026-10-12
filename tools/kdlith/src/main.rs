//! The kdlith CLI entry point.

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
kdlith - typed, streaming KDL processing and Rust processor generation

Usage: kdlith [--help | --version]
       kdlith read <file>

Commands:
  read <file>    Parse a KDL document; exit 0 when it is valid KDL, or print
                 the first diagnostic to stderr and exit nonzero

Options:
  -h, --help     Print this help and exit
  -V, --version  Print the version and exit

Other commands are not implemented yet; any other invocation fails.";

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
        [verb, file] if verb == "read" => kdlith_cli::read(Path::new(file)).into(),
        [verb, ..] if verb == "read" => {
            eprintln!("kdlith: usage: kdlith read <file>");
            ExitCode::from(2)
        }
        // Every other verb stays fail-closed until it is wired, rather
        // than pretending to work.
        _ => {
            eprintln!("kdlith: not implemented yet");
            ExitCode::from(1)
        }
    }
}
