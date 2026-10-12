//! Diagnostic codes, labels, and presentation-neutral structures: the
//! diagnostic record, the code registry, the response envelope, and exit
//! strata. The base of the kdlith dependency graph; it encodes nothing.
mod envelope;
mod exit;
mod record;
pub mod registry;

pub use envelope::{
    CoverageRecord, ENVELOPE_VERSION, Envelope, NoteRecord, OriginRecord, PaginationRecord,
    ResultKind, TruncationRecord,
};
pub use exit::ExitStatus;
pub use record::{ByteSpan, DiagnosticRecord, Label, Severity};
