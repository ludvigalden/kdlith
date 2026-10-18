//! The persisted snapshot store: per-record context snapshots, one
//! owned `<key>.kdlith-snapshot` entry per entry document. Encoding and
//! loading only; whether a snapshot restores is the restore's decision.

use std::io;
use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_runtime::{
    EncodedEntry, SnapshotEntry, SnapshotLookup, SnapshotMeta, SnapshotRecord, encode_snapshot,
    read_snapshot,
};

use crate::data::EntryContext;
use crate::definition::Definition;
use crate::snapshots::{ContextSnapshot, definition_layout_digest, slot_names};

/// What loading a record's snapshot found.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SnapshotLoad {
    /// No snapshot entry under the key, or none indexing the record.
    Miss,
    /// The record's index entry and its context snapshot.
    Hit(Box<(SnapshotEntry, SnapshotRecord, ContextSnapshot)>),
    /// An owned entry under the key that cannot be read as this
    /// version's (`KDLITH-CAC002`).
    Unusable(Box<DiagnosticRecord>),
}

/// Encode the record snapshots of `entry`'s reading under `def` as one
/// owned snapshot entry: its metadata line stamps the lookup key's
/// inputs, the fingerprint and lineage a restore checks, and the
/// payload's counts and digests; one index line per record follows, in
/// stream order.
pub fn encode_snapshot_entry(
    def: &Definition,
    entry: &EntryContext,
    snapshots: &[ContextSnapshot],
    fingerprint: &str,
) -> io::Result<EncodedEntry> {
    let lineage = entry.lineage.clone();
    let meta = SnapshotMeta::new(
        entry.entry.clone(),
        entry.revision,
        entry.profiles,
        fingerprint,
        lineage,
        definition_layout_digest(def),
    );
    let records: Vec<SnapshotRecord> = entry
        .records
        .iter()
        .zip(snapshots)
        .map(|(record, snapshot)| {
            SnapshotRecord::new(
                record.identity.clone(),
                record.ty.clone(),
                (record.revision, record.source, record.node),
                record.span,
                record.validation.name(),
                snapshot.materialize(),
            )
        })
        .collect();
    encode_snapshot(meta, &slot_names(def), &records)
}

/// Load the snapshot of the record `identity` from the entry `key` in
/// `dir`: the first indexed record carrying that identity, in stream
/// order, as the record lookup of a replayed reading finds it. A
/// missing directory, entry, or record is a miss; a read failure is
/// `KDLITH-CAC003`.
pub fn load_snapshot(
    dir: &Path,
    key: &str,
    identity: &str,
) -> Result<SnapshotLoad, Box<DiagnosticRecord>> {
    Ok(match read_snapshot(dir, key)? {
        SnapshotLookup::Hit(entry, records) => {
            let found = records
                .into_iter()
                .find(|record| record.identity.as_deref() == Some(identity));
            match found {
                Some(record) => {
                    let entry = *entry;
                    let snapshot = ContextSnapshot::persisted(
                        record.snapshot.clone(),
                        record.values.clone(),
                        entry.meta.schema.clone(),
                        entry.meta.lineage.clone(),
                    );
                    SnapshotLoad::Hit(Box::new((entry, record, snapshot)))
                }
                None => SnapshotLoad::Miss,
            }
        }
        SnapshotLookup::Unusable(record) => SnapshotLoad::Unusable(record),
        _ => SnapshotLoad::Miss,
    })
}
