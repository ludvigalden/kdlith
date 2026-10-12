//! The kdlith CLI entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    // The verb dispatch body is not wired yet; until then the tool fails
    // closed rather than pretending to work.
    eprintln!("kdlith: not implemented yet");
    ExitCode::from(1)
}
