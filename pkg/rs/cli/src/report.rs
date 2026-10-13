//! Diagnostics on stderr: one `error[CODE]: message` line per record
//! with its location, and the exit stratum the records decide.

use std::fmt::Write as _;

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};

/// Print `record` and map it to its exit stratum.
pub(crate) fn report(record: &DiagnosticRecord) -> ExitStatus {
    report_all(std::slice::from_ref(record))
}

/// Print every record in order and map them to one exit stratum.
pub(crate) fn report_all(records: &[DiagnosticRecord]) -> ExitStatus {
    for record in records {
        eprintln!("{}", render(record));
    }
    ExitStatus::from_diagnostics(records.iter().map(|r| (r.code.as_str(), r.severity)), false)
}

fn render(record: &DiagnosticRecord) -> String {
    let mut line = format!("error[{}]", record.code);
    if let Some(message) = &record.message {
        let _ = write!(line, ": {message}");
    }
    if let Some(path) = &record.source_path {
        let _ = write!(line, "\n  --> {path}");
        if let (Some(row), Some(col)) = (record.line, record.col) {
            let _ = write!(line, ":{row}:{col}");
        }
    }
    for label in &record.labels {
        let _ = write!(line, "\n  note: {}", label.message);
        if let Some(path) = &label.source_path {
            let _ = write!(line, " ({path})");
        }
    }
    line
}
