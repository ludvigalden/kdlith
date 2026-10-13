//! The kdlith CLI entry point: a thin main over `kdlith_cli::run`.

use std::process::ExitCode;

fn main() -> ExitCode {
    kdlith_cli::run().into()
}
