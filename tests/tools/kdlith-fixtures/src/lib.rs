//! The kdlith fixture harness: discovers declarative scenarios under a
//! suite root, materializes each into a private scratch directory, drives
//! the built kdlith binary there as a subprocess, and evaluates the
//! scenario's declared expectations itself.
//!
//! Harness independence: the harness never asks the processor under test
//! to interpret an expectation. Manifests are read with `serde_json`,
//! negative-control headers are read as raw text, and every assertion is
//! a plain comparison made here.

mod fxbin;
mod fxcfg;
mod harness;
mod outcome;
mod scenario;
mod scratch;
mod suite;

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
