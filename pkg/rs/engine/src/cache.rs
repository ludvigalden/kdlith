//! Deriving the persistent cache state: one data reading's bound
//! results and entry context snapshots, encoded as the store's owned
//! entries.

use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::Item;
use kdlith_runtime::{EncodedEntry, encode_entry};

use crate::data::{DataReader, ReadOptions};
use crate::snapshot_store::encode_snapshot_entry;
use crate::snapshots::snapshot_fingerprint;
use crate::typed::ReadError;

/// What one derivation produced: the entries to persist — the bound
/// results in key order, then the snapshots in key order — and every
/// diagnostic the reading reported. A reading with any error
/// diagnostic is not a state to persist; the caller fails closed.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct DerivedState {
    pub entries: Vec<EncodedEntry>,
    pub records: Vec<DiagnosticRecord>,
    pub failed: bool,
}

/// Read the entry documents and their imports, and encode each bound
/// result the reading cached as one owned entry, and each entry
/// document's records with the contexts they bound under as one
/// snapshot entry. The reading is the same
/// execution-backed one every data verb runs, so an entry holds exactly
/// what the session-level cache would replay. An entry whose reading
/// consulted the value resolver gets no snapshot entry.
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
    let (failed, results, contexts) = reader.read_state(entries, options, &mut sink)?;
    // Encoding into memory cannot fail on I/O; a serializer refusal is a
    // broken item contract, reported as such.
    let mut encoded = Vec::with_capacity(results.len());
    for result in results {
        let entry = encode_entry(result.identity, result.derivations, &result.items)
            .map_err(|_| ReadError::UnsupportedItem)?;
        encoded.push(entry);
    }
    encoded.sort_by(|a, b| a.entry.key().cmp(b.entry.key()));
    let def = reader.definition();
    let fingerprint = snapshot_fingerprint(def);
    let mut snapshots = Vec::new();
    for (entry, records) in reader.record_snapshots(contexts) {
        if let Some(records) = records {
            let encoded = encode_snapshot_entry(def, &entry, &records, &fingerprint)
                .map_err(|_| ReadError::UnsupportedItem)?;
            snapshots.push(encoded);
        }
    }
    snapshots.sort_by(|a, b| a.entry.key().cmp(b.entry.key()));
    encoded.extend(snapshots);
    Ok(DerivedState {
        entries: encoded,
        records,
        failed,
    })
}
