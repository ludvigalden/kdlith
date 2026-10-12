//! The kdlith fixture harness: drives the committed fixture suite under
//! `tests/fixtures/` against the freshly built kdlith binary.

use std::process::ExitCode;

fn main() -> ExitCode {
    // The harness implementation is not wired yet; until then it fails
    // closed rather than reporting an empty suite green.
    eprintln!("kdlith-fixtures: not implemented yet");
    ExitCode::from(1)
}
