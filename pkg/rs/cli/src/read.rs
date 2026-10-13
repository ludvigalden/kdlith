//! `kdlith read <file>`: parse one document to its clean end and report
//! the first failure as a registered diagnostic on stderr.

use std::path::Path;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_parse::Parser;

use crate::report::report;

/// Read `path` and drain its pull-event stream. Success prints nothing;
/// a failure prints one diagnostic line to stderr and fails with the
/// code's exit stratum.
pub fn read(path: &Path) -> ExitStatus {
    let shown = path.display().to_string();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return report(&source_error(
                "KDLITH-SRC001",
                &[&shown, &error.to_string()],
            ));
        }
    };
    let source = match std::str::from_utf8(&bytes) {
        Ok(source) => source,
        Err(error) => {
            let at = error.valid_up_to().to_string();
            return report(&source_error("KDLITH-SRC002", &[&shown, &at]));
        }
    };
    let mut parser = Parser::new(source);
    loop {
        match parser.next_event() {
            Ok(Some(_)) => {}
            Ok(None) => return ExitStatus::Success,
            Err(error) => return report(&error.to_diagnostic(source).with_source(shown)),
        }
    }
}

/// A source-area record with its registered template filled.
fn source_error(code: &str, values: &[&str]) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(code).expect("source codes are registered and live");
    match lookup(code) {
        Some(definition) => record.with_message(render(definition.message_template, values)),
        None => record,
    }
}
