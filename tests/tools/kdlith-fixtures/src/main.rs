//! The kdlith fixture harness binary: a thin entry point over the
//! harness library. Under an `fx-*` name it runs that helper instead.

use std::process::ExitCode;

fn main() -> ExitCode {
    kdlith_fixtures::main_entry(std::env::args_os().collect())
}
