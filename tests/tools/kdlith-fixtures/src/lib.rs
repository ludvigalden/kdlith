//! The kdlith fixture harness: discovers declarative scenarios under
//! a suite root, drives the built kdlith binary as a subprocess in a
//! private scratch directory, and evaluates expectations itself.

mod cleanrun;
mod fxbin;
mod fxcfg;
mod harness;
mod oracle;
mod outcome;
mod scenario;
mod scratch;
mod suite;
mod walk;

use std::ffi::OsString;
use std::process::ExitCode;

/// The process entry point: dispatch an `fx-*` helper when invoked under a
/// helper name, otherwise run the harness CLI.
pub fn main_entry(argv: Vec<OsString>) -> ExitCode {
    let argv0 = argv.first().cloned().unwrap_or_default();
    if let Some(code) = fxbin::dispatch_helper(&argv0) {
        return code;
    }
    let argv: Vec<String> = match argv.into_iter().map(OsString::into_string).collect() {
        Ok(a) => a,
        Err(bad) => {
            eprintln!("kdlith-fixtures: argument is not valid UTF-8: {bad:?}");
            return harness::EXIT_USAGE.into();
        }
    };
    let code = harness::run(&argv);
    ExitCode::from(code)
}
