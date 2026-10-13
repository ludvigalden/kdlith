//! Reading document text for processing: the size bound checked before
//! anything is read, UTF-8 decoding, and import failures, each as its
//! registered diagnostic.

use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_runtime::ImportFailure;

use crate::diag::{
    IMPORT_CYCLE, IMPORT_REPEATED, IMPORT_UNRESOLVED, SOURCE_LIMIT, SOURCE_NOT_UTF8,
    SOURCE_UNREADABLE, record,
};

/// A document larger than this is refused before it is read.
pub const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;

/// The text of the document at `path`, shown as `shown`. `limit` names
/// the size bound in its diagnostic; `origin` is the document's role.
pub(crate) fn read_text(
    path: &Path,
    shown: &str,
    limit: &str,
    origin: &str,
) -> Result<String, Box<DiagnosticRecord>> {
    let located = |record: DiagnosticRecord| Box::new(record.with_source(shown));
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_DOCUMENT_BYTES
    {
        let max = MAX_DOCUMENT_BYTES.to_string();
        return Err(located(record(SOURCE_LIMIT, &[limit, &max], origin)));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        let reason = error.to_string();
        located(record(SOURCE_UNREADABLE, &[shown, &reason], origin))
    })?;
    String::from_utf8(bytes).map_err(|error| {
        let at = error.utf8_error().valid_up_to().to_string();
        located(record(SOURCE_NOT_UTF8, &[shown, &at], origin))
    })
}

/// An import the invocation stack refused, as its registered diagnostic.
pub(crate) fn import_failure(failure: ImportFailure, origin: &str) -> DiagnosticRecord {
    match failure {
        ImportFailure::Unresolved { target, .. } => record(IMPORT_UNRESOLVED, &[&target], origin),
        ImportFailure::Cycle { chain } => record(IMPORT_CYCLE, &[&chain.join(" -> ")], origin),
        ImportFailure::Repeated { target, .. } => record(IMPORT_REPEATED, &[&target], origin),
        ImportFailure::Limit { limit, max } => {
            record(SOURCE_LIMIT, &[limit, &max.to_string()], origin)
        }
        ImportFailure::Projection(error) => {
            record(IMPORT_UNRESOLVED, &[&error.to_string()], origin)
        }
        _ => record(IMPORT_UNRESOLVED, &["(unknown request)"], origin),
    }
}
