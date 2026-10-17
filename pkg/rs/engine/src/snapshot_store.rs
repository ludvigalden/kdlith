//! The persisted snapshot store: entry context snapshots as owned
//! `<key>.kdlith-snapshot` entries of the cache's state directory.
//! Encoding and loading only; whether a snapshot restores is the
//! restore's decision.

use std::io;
use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_runtime::{
    EncodedEntry, SnapshotEntry, SnapshotLookup, SnapshotMeta, encode_snapshot, read_snapshot,
};

use crate::data::EntryContext;
use crate::definition::Definition;
use crate::snapshots::{ContextSnapshot, definition_layout_digest, snapshot_payload};

/// What loading a snapshot by key found.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SnapshotLoad {
    /// No snapshot entry under the key.
    Miss,
    /// The persisted snapshot and the entry it was loaded from.
    Hit(Box<(SnapshotEntry, ContextSnapshot)>),
    /// An owned entry under the key that cannot be read as this
    /// version's (`KDLITH-CAC002`).
    Unusable(Box<DiagnosticRecord>),
}

/// Encode `snapshot`, captured from `entry` under `def`, as one owned
/// snapshot entry: its metadata line stamps the lookup key's inputs,
/// the fingerprint and lineage a restore checks, and the payload's
/// layout and body digests.
pub fn encode_snapshot_entry(
    def: &Definition,
    entry: &EntryContext,
    snapshot: &ContextSnapshot,
) -> io::Result<EncodedEntry> {
    let meta = SnapshotMeta::new(
        entry.entry.clone(),
        entry.revision,
        entry.profiles,
        snapshot.fingerprint(),
        snapshot.lineage().to_vec(),
        definition_layout_digest(def),
    );
    encode_snapshot(meta, &snapshot_payload(def, &snapshot.materialize()))
}

/// Load the snapshot entry `key` from `dir`. A missing directory or
/// entry is a miss; a read failure is `KDLITH-CAC003`.
pub fn load_snapshot(dir: &Path, key: &str) -> Result<SnapshotLoad, Box<DiagnosticRecord>> {
    Ok(match read_snapshot(dir, key)? {
        SnapshotLookup::Hit(entry, values) => {
            let entry = *entry;
            let snapshot = ContextSnapshot::persisted(
                entry.key.clone(),
                values,
                entry.meta.schema.clone(),
                entry.meta.lineage.clone(),
            );
            SnapshotLoad::Hit(Box::new((entry, snapshot)))
        }
        SnapshotLookup::Unusable(record) => SnapshotLoad::Unusable(record),
        _ => SnapshotLoad::Miss,
    })
}
