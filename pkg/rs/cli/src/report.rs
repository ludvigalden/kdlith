//! Diagnostics on stderr: one `<severity>[CODE]: message` line per
//! record with its location, and the exit stratum the records decide;
//! `--diagnostics json` also mirrors each to stdout as a JSON line.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};

/// Whether a warning fails the run, as `--warnings-as-errors` says.
static WARNINGS_AS_ERRORS: AtomicBool = AtomicBool::new(false);

/// Whether every printed diagnostic is also written to stdout as one
/// compact JSON line, as `--diagnostics json` arms on the data verbs.
static DIAGNOSTICS_JSON: AtomicBool = AtomicBool::new(false);

/// Set whether a warning fails the run this command decides.
pub(crate) fn fail_on_warnings(on: bool) {
    WARNINGS_AS_ERRORS.store(on, Ordering::Relaxed);
}

/// Arm or disarm the stdout JSON mirror of every printed diagnostic.
pub(crate) fn mirror_diagnostics_as_json(on: bool) {
    DIAGNOSTICS_JSON.store(on, Ordering::Relaxed);
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
/// An armed mirror writes each record to stdout as one compact JSON
/// line, in print order; write errors there are ignored, as the stderr
/// write's are.
pub(crate) fn print_notes(records: &[DiagnosticRecord]) {
    let mirror = DIAGNOSTICS_JSON.load(Ordering::Relaxed);
    for record in records {
        eprintln!("{}", record.render());
        if mirror {
            let mut out = std::io::stdout().lock();
            let _ = serde_json::to_writer(&mut out, record);
            let _ = out.write_all(b"\n");
        }
    }
}
