//! Deriving the persistent cache state: one data reading's bound
//! results, encoded as the store's owned entries.

use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::Item;
use kdlith_runtime::{EncodedEntry, encode_entry};

use crate::data::{DataReader, ReadOptions};
use crate::typed::ReadError;

/// What one derivation produced: the entries to persist, in key order,
/// and every diagnostic the reading reported. A reading with any error
/// diagnostic is not a state to persist; the caller fails closed.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct DerivedState {
    pub entries: Vec<EncodedEntry>,
    pub records: Vec<DiagnosticRecord>,
    pub failed: bool,
}

/// Read the entry documents and their imports, and encode each bound
/// result the reading cached as one owned entry. The reading is the
/// same execution-backed one every data verb runs, so an entry holds
/// exactly what the session-level cache would replay.
pub fn derive_state(
    reader: &DataReader<'_>,
    entries: &[&Path],
    options: &ReadOptions,
) -> Result<DerivedState, ReadError> {
    let mut records = Vec::new();
    let mut sink = |item: Item| {
        if let Item::Diagnostic(record) = item {
            records.push(*record);
        }
    };
    let (failed, results) = reader.read_results(entries, options, &mut sink)?;
    let mut encoded = Vec::with_capacity(results.len());
    for result in results {
        // Encoding into memory cannot fail on I/O; a serializer refusal
        // is a broken item contract, reported as such.
        let entry = encode_entry(result.identity, result.derivations, &result.items)
            .map_err(|_| ReadError::UnsupportedItem)?;
        encoded.push(entry);
    }
    encoded.sort_by(|a, b| a.entry.key.cmp(&b.entry.key));
    Ok(DerivedState {
        entries: encoded,
        records,
        failed,
    })
}
