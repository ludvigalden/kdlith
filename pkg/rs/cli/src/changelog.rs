//! `kdlith changelog [--target TARGET]`: print one of the relmap
//! export targets this binary embedded when it was built (see the
//! build script). A build without relmap embeds none, and the verb
//! reports them unavailable.

use std::io::Write as _;

use clap::ValueEnum;
use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};

use crate::report::report;

/// The version this build presents: relmap's for the kdlith binary
/// when the build could ask relmap, else the Cargo version.
pub(crate) const PRESENTED_VERSION: &str = match option_env!("KDLITH_PRESENTED_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// The relmap export target `changelog --target` selects; the default
/// keeps the verb's previous output, the markdown page.
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(crate) enum Target {
    /// The markdown page, the same text as the root CHANGELOG.md
    #[default]
    Markdown,
    /// The newline-delimited JSON records
    Ndjson,
    /// The record index over the NDJSON bytes
    Index,
    /// The per-line metadata
    Metadata,
}

#[cfg(kdlith_changelog)]
fn notes(target: Target) -> Option<&'static [u8]> {
    Some(match target {
        Target::Markdown => {
            include_bytes!(concat!(env!("OUT_DIR"), "/relmap/CHANGELOG.md")).as_slice()
        }
        Target::Ndjson => {
            include_bytes!(concat!(env!("OUT_DIR"), "/relmap/changelog.ndjson")).as_slice()
        }
        Target::Index => {
            include_bytes!(concat!(env!("OUT_DIR"), "/relmap/changelog.index.json")).as_slice()
        }
        Target::Metadata => {
            include_bytes!(concat!(env!("OUT_DIR"), "/relmap/changelog.metadata.json")).as_slice()
        }
    })
}
#[cfg(not(kdlith_changelog))]
fn notes(_: Target) -> Option<&'static [u8]> {
    None
}

const UNAVAILABLE: &str = "KDLITH-CHG001";

/// Write the embedded target to stdout unchanged; a failed write fails
/// the run. Without embedded notes, report them unavailable.
pub(crate) fn run(target: Target) -> ExitStatus {
    let Some(notes) = notes(target) else {
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
