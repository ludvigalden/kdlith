//! Diagnostics on stderr: one `<severity>[CODE]: message` line per
//! record with its location, and the exit stratum the records decide.

use std::sync::atomic::{AtomicBool, Ordering};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};

/// Whether a warning fails the run, as `--warnings-as-errors` says.
static WARNINGS_AS_ERRORS: AtomicBool = AtomicBool::new(false);

/// Set whether a warning fails the run this command decides.
pub(crate) fn fail_on_warnings(on: bool) {
    WARNINGS_AS_ERRORS.store(on, Ordering::Relaxed);
}

/// Print `record` and map it to its exit stratum.
pub(crate) fn report(record: &DiagnosticRecord) -> ExitStatus {
    report_all(std::slice::from_ref(record))
}

/// Print every record in order and map them to one exit stratum.
pub(crate) fn report_all(records: &[DiagnosticRecord]) -> ExitStatus {
    print_notes(records);
    let strict = WARNINGS_AS_ERRORS.load(Ordering::Relaxed);
    ExitStatus::from_diagnostics(
        records.iter().map(|r| (r.code.as_str(), r.severity)),
        strict,
    )
}

/// Print records without deciding an exit, as advisory records are.
pub(crate) fn print_notes(records: &[DiagnosticRecord]) {
    for record in records {
        eprintln!("{}", record.render());
    }
}
