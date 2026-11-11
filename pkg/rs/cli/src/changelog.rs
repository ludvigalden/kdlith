//! `kdlith changelog`: print the release notes relmap rendered when
//! this binary was built (see the build script). A build without
//! relmap embeds none, and the verb reports them unavailable.

use std::io::Write as _;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};

use crate::report::report;

/// The version this build presents: relmap's for the kdlith binary
/// when the build could ask relmap, else the Cargo version.
pub(crate) const PRESENTED_VERSION: &str = match option_env!("KDLITH_PRESENTED_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[cfg(kdlith_changelog)]
const NOTES: Option<&[u8]> = Some(include_bytes!(concat!(
    env!("OUT_DIR"),
    "/relmap/CHANGELOG.md"
)));
#[cfg(not(kdlith_changelog))]
const NOTES: Option<&[u8]> = None;

const UNAVAILABLE: &str = "KDLITH-CHG001";

/// Write the embedded notes to stdout unchanged; a failed write fails
/// the run. Without embedded notes, report them unavailable.
pub(crate) fn run() -> ExitStatus {
    let Some(notes) = NOTES else {
        return unavailable();
    };
    let mut out = std::io::stdout().lock();
    if out.write_all(notes).is_err() || out.flush().is_err() {
        eprintln!("kdlith: cannot write the changelog to stdout");
        return ExitStatus::Failure;
    }
    ExitStatus::Success
}

fn unavailable() -> ExitStatus {
    let definition = lookup(UNAVAILABLE).expect("the changelog code is registered");
    let record = DiagnosticRecord::from_code(UNAVAILABLE)
        .expect("the changelog code is live")
        .with_message(render(
            definition.message_template,
            &["this kdlith was built without relmap"],
        ))
        .with_correction("Rebuild kdlith with relmap on PATH, or with RELMAP_BIN naming it");
    report(&record)
}
