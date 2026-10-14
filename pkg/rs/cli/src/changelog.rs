//! `kdlith changelog`: print the embedded changelog archive.
//! The archive is committed data compiled into the binary; the verb
//! never reads the filesystem or derives content at runtime.

use std::io::Write as _;

use kdlith_diagnostic::ExitStatus;

/// The committed, versioned changelog archive, byte for byte.
const ARCHIVE: &str = include_str!("../../../kdl/CHANGELOG.md");

/// Write the archive to stdout unchanged; a failed write fails the run.
pub(crate) fn run() -> ExitStatus {
    let mut out = std::io::stdout().lock();
    if out.write_all(ARCHIVE.as_bytes()).is_err() || out.flush().is_err() {
        eprintln!("kdlith: cannot write the changelog to stdout");
        return ExitStatus::Failure;
    }
    ExitStatus::Success
}
