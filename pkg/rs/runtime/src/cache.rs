//! The persistent derived-state store behind `kdlith cache`: one
//! versioned, owned entry file per cached bound result and per entry
//! context snapshot, in a state directory every operation names
//! explicitly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};
use kdlith_model::{OriginId, Repeated, Value};
use serde::{Deserialize, Serialize};

use crate::context::{SlotValue, stable_hash, stable_hash_step};
use crate::export::write_ndjson;
use crate::imports::InvocationIdentity;

/// The owned entry classes: a file is owned when it is a regular file
/// named `<key><suffix>` whose first line starts with its class's
/// marker; anything else in the directory is foreign — reported, never
/// read further, never removed.
const CLASSES: [(EntryClass, &str, &str); 2] = [
    (EntryClass::Cache, ".kdlith-cache", "kdlith-cache "),
    (EntryClass::Snapshot, ".kdlith-snapshot", "kdlith-snapshot "),
];

/// The entry format version this build reads and writes.
pub const CACHE_FORMAT: u32 = 3;
/// The item canonicalization version: how bound items are serialized
/// into an entry. A change to the serialized item shape takes a new
/// number, so an entry written under another one is never misread.
pub const CACHE_CANONICALIZATION: u32 = 1;
/// The snapshot entry format version this build reads and writes:
/// format 2 indexes one context snapshot per record, where format 1
/// held one per entry document.
pub const SNAPSHOT_FORMAT: u32 = 2;
/// How record index lines and their slot values are serialized into a
/// snapshot entry's payload.
pub const SNAPSHOT_CANONICALIZATION: u32 = 2;
/// The largest entry file the store reads; a larger one is corrupt.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

const FOREIGN: &str = "KDLITH-CAC001";
const CORRUPT: &str = "KDLITH-CAC002";
const INACCESSIBLE: &str = "KDLITH-CAC003";

/// The kind of owned entry a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EntryClass {
    /// One cached bound result.
    Cache,
    /// One entry document's context snapshot.
    Snapshot,
}

impl EntryClass {
    /// The file name an entry of this class holding `key` has.
    pub fn file_name(self, key: &str) -> String {
        format!("{key}{}", self.suffix())
    }

    /// Whether `bytes` begin with this class's ownership marker.
    pub fn marks(self, bytes: &[u8]) -> bool {
        bytes.starts_with(self.magic().as_bytes())
    }

    /// The ownership header this build writes for the class.
    pub fn header(self) -> String {
        let (format, canonicalization) = match self {
            EntryClass::Cache => (CACHE_FORMAT, CACHE_CANONICALIZATION),
            EntryClass::Snapshot => (SNAPSHOT_FORMAT, SNAPSHOT_CANONICALIZATION),
        };
        format!(
            "{}format={format} canonicalization={canonicalization}\n",
            self.magic()
        )
    }

    fn suffix(self) -> &'static str {
        CLASSES
            .iter()
            .find(|(class, _, _)| *class == self)
            .map_or("", |(_, suffix, _)| suffix)
    }

    fn magic(self) -> &'static str {
        CLASSES
            .iter()
            .find(|(class, _, _)| *class == self)
            .map_or("", |(_, _, magic)| magic)
    }

    fn part_name(self, key: &str) -> String {
        format!(".{key}{}-part", self.suffix())
    }
}

/// Why an entry holds what it holds: the entrypoint whose reading
/// derived it, the document it binds, and the document that imported
/// that one (`None` when the source is the entrypoint itself).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Derivation {
    pub entry: String,
    pub source: String,
    pub caller: Option<String>,
}

impl Derivation {
    pub fn new(
        entry: impl Into<String>,
        source: impl Into<String>,
        caller: Option<String>,
    ) -> Self {
        Self {
            entry: entry.into(),
            source: source.into(),
            caller,
        }
    }
}

/// One owned, intact entry: its key, the invocation identity it caches,
/// its derivations, and how many bound items it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheEntry {
    pub key: String,
    pub identity: InvocationIdentity,
    /// The source-set fingerprint of the reading that derived the entry;
    /// `None` when it used no named source.
    pub sources: Option<String>,
    pub derivations: Vec<Derivation>,
    pub items: usize,
}

impl fmt::Display for CacheEntry {
    /// The entry line, then one indented line per derivation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = &self.identity;
        write!(
            f,
            "cache {} revision={} schema={} context={} profiles={} resolved={} policy={} items={}",
            self.key,
            digest(id.revision),
            digest(id.schema),
            digest(id.context),
            digest(id.profiles),
            digest(id.resolved),
            policy_name(id.policy),
            self.items
        )?;
        if let Some(sources) = &self.sources {
            write!(f, " sources={sources}")?;
        }
        for d in &self.derivations {
            write!(
                f,
                "\n  derived entry={} source={} caller={}",
                d.entry,
                d.source,
                d.caller.as_deref().unwrap_or("-")
            )?;
        }
        Ok(())
    }
}

/// One owned, intact snapshot entry: its key, the entry document whose
/// records it indexes, and the stamps its metadata line carries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SnapshotEntry {
    pub key: String,
    pub meta: SnapshotMeta,
}

impl fmt::Display for SnapshotEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = &self.meta;
        write!(
            f,
            "snapshot {} entry={} revision={} schema={} profiles={} lineage=[{}] records={} slots={} layout={}",
            self.key,
            meta.entry,
            meta.revision,
            meta.schema,
            meta.profiles,
            meta.lineage.join(","),
            meta.records,
            meta.slots,
            meta.layout
        )?;
        if !meta.sources.is_empty() {
            let pairs: Vec<String> = meta
                .sources
                .iter()
                .map(|(name, identity)| format!("{name}={identity}"))
                .collect();
            write!(f, " sources=[{}]", pairs.join(","))?;
        }
        Ok(())
    }
}

/// The metadata line of a snapshot entry: the lookup key's inputs (the
/// entry document as its invocation spelled it, its source revision,
/// the profile selection), the schema fingerprint and lineage a restore
/// checks, the record count, each record's slot count, the slot-layout
/// digest, and the body digest. Every fingerprint is a stamped digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SnapshotMeta {
    pub key: String,
    pub entry: String,
    pub revision: String,
    pub schema: String,
    pub profiles: String,
    pub lineage: Vec<String>,
    /// Each named source the snapshotted reading used, by name, with the
    /// identity of the tree it read; a restore under any other identity
    /// is a miss. Empty when it used none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, String>,
    pub records: usize,
    pub slots: usize,
    pub layout: String,
    pub digest: String,
}

impl SnapshotMeta {
    /// The metadata of a snapshot of `entry` at `revision` under the
    /// profile selection `profiles`; the key, the counts, and the body
    /// digest are derived, never supplied.
    pub fn new(
        entry: impl Into<String>,
        revision: u64,
        profiles: u64,
        schema: impl Into<String>,
        lineage: Vec<String>,
        layout: impl Into<String>,
    ) -> Self {
        let entry = entry.into();
        Self {
            key: snapshot_key(&entry, revision, profiles),
            entry,
            revision: digest(revision),
            schema: schema.into(),
            profiles: digest(profiles),
            lineage,
            sources: BTreeMap::new(),
            records: 0,
            slots: 0,
            layout: layout.into(),
            digest: String::new(),
        }
    }
}

impl SnapshotMeta {
    /// The metadata with the named sources the reading used and the
    /// identity of each tree it read.
    #[must_use]
    pub fn with_sources(mut self, sources: BTreeMap<String, String>) -> Self {
        self.sources = sources;
        self
    }
}

/// One owned, intact entry of either class.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OwnedEntry {
    Cache(CacheEntry),
    Snapshot(SnapshotEntry),
}

impl OwnedEntry {
    pub fn class(&self) -> EntryClass {
        match self {
            OwnedEntry::Cache(_) => EntryClass::Cache,
            OwnedEntry::Snapshot(_) => EntryClass::Snapshot,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            OwnedEntry::Cache(entry) => &entry.key,
            OwnedEntry::Snapshot(entry) => &entry.key,
        }
    }

    /// The entry's file name in the state directory.
    pub fn file_name(&self) -> String {
        self.class().file_name(self.key())
    }
}

impl fmt::Display for OwnedEntry {
    /// The entry's line, led by its class label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnedEntry::Cache(entry) => entry.fmt(f),
            OwnedEntry::Snapshot(entry) => entry.fmt(f),
        }
    }
}

/// Why an owned file is not an intact entry. A version mismatch is its
/// own outcome, distinct from damage, so a newer or older store's entry
/// is never misread as this one's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryFault {
    /// The ownership header names another format or canonicalization
    /// than `current`, the one this build reads for the entry's class.
    Version { header: String, current: String },
    /// A write was interrupted and left its hidden part file.
    Interrupted,
    /// The entry fails a structural check.
    Damaged(String),
}

impl fmt::Display for EntryFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryFault::Version { header, current } => {
                write!(f, "written as {header:?}, not this version's {current:?}")
            }
            EntryFault::Interrupted => f.write_str("an interrupted write left this partial entry"),
            EntryFault::Damaged(reason) => f.write_str(reason),
        }
    }
}

/// What a scan of a state directory found. Every owned file is either
/// an intact entry or corrupt; everything else is foreign.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct CacheScan {
    /// `false` when the state directory does not exist.
    pub present: bool,
    /// Intact entries, cache entries first, each class sorted by key.
    pub entries: Vec<OwnedEntry>,
    /// Owned files that fail their envelope check, by file name, with
    /// the reason; sorted by file name.
    pub corrupt: Vec<(String, EntryFault)>,
    /// Entries the store does not own, by file name; sorted.
    pub foreign: Vec<String>,
}

impl CacheScan {
    /// One `KDLITH-CAC002` per corrupt entry, then one `KDLITH-CAC001`
    /// per foreign entry, each naming its path under `dir`.
    pub fn records(&self, dir: &Path) -> Vec<DiagnosticRecord> {
        let mut records = Vec::new();
        for (name, reason) in &self.corrupt {
            let path = dir.join(name).display().to_string();
            records.push(coded(CORRUPT, &[&path, &reason.to_string()]));
        }
        for name in &self.foreign {
            let path = dir.join(name).display().to_string();
            records.push(coded(FOREIGN, &[&path]));
        }
        records
    }

    /// Whether every entry in the directory is owned and intact.
    pub fn is_clean(&self) -> bool {
        self.corrupt.is_empty() && self.foreign.is_empty()
    }

    /// The owned file names, intact and corrupt: everything the store
    /// may remove.
    fn owned(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .map(OwnedEntry::file_name)
            .chain(self.corrupt.iter().map(|(name, _)| name.clone()))
            .collect()
    }
}

/// One entry ready to write: what it is and its encoded bytes.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct EncodedEntry {
    pub entry: OwnedEntry,
    bytes: Vec<u8>,
}

/// The key of an invocation identity: a stable FNV-1a hash of its six
/// components, as 16 lowercase hex digits.
pub fn cache_key(identity: &InvocationIdentity) -> String {
    cache_key_under(identity, None)
}

/// The key of an invocation identity bound by a reading whose named
/// sources fingerprint to `sources`: the identity's key with the
/// fingerprint folded in, so one document read against two different
/// trees of one source name never shares a key. `None` (no named source
/// used) keys exactly as [`cache_key`].
pub fn cache_key_under(identity: &InvocationIdentity, sources: Option<&str>) -> String {
    let mut hash = stable_hash(b"kdlith-cache-key");
    hash = stable_hash_step(hash, &identity.revision.to_le_bytes());
    hash = stable_hash_step(hash, &identity.schema.to_le_bytes());
    hash = stable_hash_step(hash, &identity.context.to_le_bytes());
    hash = stable_hash_step(hash, &identity.profiles.to_le_bytes());
    hash = stable_hash_step(hash, &identity.resolved.to_le_bytes());
    hash = stable_hash_step(hash, policy_name(identity.policy).as_bytes());
    if let Some(sources) = sources {
        hash = stable_hash_step(hash, b"\0sources\0");
        hash = stable_hash_step(hash, sources.as_bytes());
    }
    format!("{hash:016x}")
}

/// The key of a snapshot: a stable FNV-1a hash of the entry document as
/// its invocation spelled it (length-prefixed), its source revision, and
/// its profile-selection fingerprint. The schema fingerprint and the
/// lineage are deliberately not part of it: they travel in the entry and
/// are checked by the restore, so a mismatch is reported, never a miss.
pub fn snapshot_key(entry: &str, revision: u64, profiles: u64) -> String {
    let mut hash = stable_hash(b"kdlith-snapshot-key");
    hash = stable_hash_step(hash, &(entry.len() as u64).to_le_bytes());
    hash = stable_hash_step(hash, entry.as_bytes());
    hash = stable_hash_step(hash, &revision.to_le_bytes());
    hash = stable_hash_step(hash, &profiles.to_le_bytes());
    format!("{hash:016x}")
}

/// One indexed record of a snapshot entry: the record's identity (its
/// `id` field, when it has one) and type, where it was bound (the
/// source's revision digest, its source id within the reading, the
/// node's index, and the byte span the record reports), the id of its
/// context snapshot, how far its checking had progressed, and the
/// context it bound under, in slot-id order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SnapshotRecord {
    pub identity: Option<String>,
    pub ty: String,
    pub digest: String,
    pub source: u32,
    pub node: u32,
    pub span: (u32, u32),
    pub snapshot: String,
    pub validation: String,
    pub values: Vec<Option<SlotValue>>,
}

impl SnapshotRecord {
    /// A record bound at `node` of `source`, whose text has revision
    /// `revision`; its snapshot id is assigned when it is encoded.
    pub fn new(
        identity: Option<String>,
        ty: impl Into<String>,
        (revision, source, node): (u64, u32, u32),
        span: (u32, u32),
        validation: impl Into<String>,
        values: Vec<Option<SlotValue>>,
    ) -> Self {
        Self {
            identity,
            ty: ty.into(),
            digest: digest(revision),
            source,
            node,
            span,
            snapshot: String::new(),
            validation: validation.into(),
            values,
        }
    }
}

/// The validation statuses a record index line may name.
const VALIDATIONS: [&str; 3] = ["validated", "pending", "failed"];

/// Encode one snapshot entry: the ownership header, the metadata line
/// with its counts and body digest stamped from the records, then one
/// index line per record in the order given. `names` names the slots;
/// each record's snapshot id is `<key>/<position>`.
pub fn encode_snapshot(
    mut meta: SnapshotMeta,
    names: &[String],
    records: &[SnapshotRecord],
) -> io::Result<EncodedEntry> {
    let mut body = Vec::new();
    let mut indexed = Vec::with_capacity(records.len());
    for (at, record) in records.iter().enumerate() {
        let mut record = record.clone();
        record.snapshot = format!("{}/{at}", meta.key);
        write_ndjson(&mut body, &RecordLine::from_record(names, &record))?;
        indexed.push(record);
    }
    meta.records = records.len();
    meta.slots = names.len();
    meta.digest = snapshot_digest(&body);
    let mut bytes = EntryClass::Snapshot.header().into_bytes();
    write_ndjson(&mut bytes, &meta)?;
    bytes.extend_from_slice(&body);
    Ok(EncodedEntry {
        entry: OwnedEntry::Snapshot(SnapshotEntry {
            key: meta.key.clone(),
            meta,
        }),
        bytes,
    })
}

/// A stamped digest of a slot layout: each slot's name in slot-id
/// order, length-prefixed, under its own hash domain.
pub fn slot_layout_digest<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut hash = stable_hash(b"kdlith-slot-layout");
    for name in names {
        hash = stable_hash_step(hash, &(name.len() as u64).to_le_bytes());
        hash = stable_hash_step(hash, name.as_bytes());
    }
    digest(hash)
}

/// One record index line of a snapshot payload.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordLine {
    identity: Option<String>,
    #[serde(rename = "type")]
    ty: String,
    digest: String,
    source: u32,
    node: u32,
    span: SpanLine,
    snapshot: String,
    validation: String,
    slots: Vec<SlotLine>,
}

/// A record's byte span in its source.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanLine {
    start: u32,
    end: u32,
}

/// One slot of a record's context: its name, its value in the tagged
/// form, and its layer; an unset slot's value and layer are null.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotLine {
    name: String,
    value: Option<Value>,
    layer: Option<String>,
}

impl RecordLine {
    fn from_record(names: &[String], record: &SnapshotRecord) -> Self {
        let slots = record
            .values
            .iter()
            .enumerate()
            .map(|(at, held)| SlotLine {
                name: names.get(at).cloned().unwrap_or_default(),
                value: held.as_ref().map(|held| held.value.clone()),
                layer: held.as_ref().and_then(|held| held.layer.clone()),
            })
            .collect();
        Self {
            identity: record.identity.clone(),
            ty: record.ty.clone(),
            digest: record.digest.clone(),
            source: record.source,
            node: record.node,
            span: SpanLine {
                start: record.span.0,
                end: record.span.1,
            },
            snapshot: record.snapshot.clone(),
            validation: record.validation.clone(),
            slots,
        }
    }

    /// The record the line indexes, with its slot names; values come
    /// back untraced, each with the layer it was captured under.
    fn into_record(self) -> Result<(SnapshotRecord, Vec<String>), String> {
        if !VALIDATIONS.contains(&self.validation.as_str()) {
            return Err(format!("unknown validation status {:?}", self.validation));
        }
        undigest(&self.digest)?;
        let mut names = Vec::with_capacity(self.slots.len());
        let mut values = Vec::with_capacity(self.slots.len());
        for slot in self.slots {
            values.push(match (slot.value, slot.layer) {
                (Some(value), layer) => {
                    Some(SlotValue::new(value, OriginId::SOURCE_METADATA).with_layer(layer))
                }
                (None, None) => None,
                (None, Some(_)) => {
                    return Err(format!("unset slot {:?} carries a layer", slot.name));
                }
            });
            names.push(slot.name);
        }
        let record = SnapshotRecord {
            identity: self.identity,
            ty: self.ty,
            digest: self.digest,
            source: self.source,
            node: self.node,
            span: (self.span.start, self.span.end),
            snapshot: self.snapshot,
            validation: self.validation,
            values,
        };
        Ok((record, names))
    }
}

/// What looking a snapshot up by key found.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SnapshotLookup {
    /// No owned snapshot entry with the key exists.
    Miss,
    /// An intact entry: its metadata and its indexed records, in the
    /// order they were bound.
    Hit(Box<SnapshotEntry>, Vec<SnapshotRecord>),
    /// An owned entry that cannot be used: another version, an
    /// interrupted write, or damage — reported as `KDLITH-CAC002`, never
    /// read as this version's.
    Unusable(Box<DiagnosticRecord>),
}

/// Look up the snapshot entry `key` in `dir`. A missing directory or
/// file is a miss; a file under the entry's name that is not
/// snapshot-owned is a miss too, since the store never reads a foreign
/// file further. A read failure is `KDLITH-CAC003`.
pub fn read_snapshot(dir: &Path, key: &str) -> Result<SnapshotLookup, Box<DiagnosticRecord>> {
    if !is_key(key) {
        return Ok(SnapshotLookup::Miss);
    }
    let name = EntryClass::Snapshot.file_name(key);
    let path = dir.join(&name);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Ok(SnapshotLookup::Miss),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SnapshotLookup::Miss);
        }
        Err(error) => {
            return Err(inaccessible(
                "read",
                &path.display().to_string(),
                &error.to_string(),
            ));
        }
    }
    let bytes = read_capped(&path)?;
    if !EntryClass::Snapshot.marks(&bytes) {
        return Ok(SnapshotLookup::Miss);
    }
    Ok(match decode_snapshot(key, &bytes) {
        Ok((entry, records)) => SnapshotLookup::Hit(Box::new(entry), records),
        Err(fault) => {
            let shown = path.display().to_string();
            SnapshotLookup::Unusable(Box::new(coded(CORRUPT, &[&shown, &fault.to_string()])))
        }
    })
}

/// Encode one entry: the ownership header, one metadata line, then one
/// NDJSON line per bound item. Derivations are sorted and deduplicated,
/// so equal inputs encode to equal bytes.
pub fn encode_entry<T: Serialize>(
    identity: InvocationIdentity,
    derivations: Vec<Derivation>,
    items: &[T],
) -> io::Result<EncodedEntry> {
    encode_entry_under(identity, None, derivations, items)
}

/// [`encode_entry`] for a result bound by a reading whose named sources
/// fingerprint to `sources`, keyed by [`cache_key_under`].
pub fn encode_entry_under<T: Serialize>(
    identity: InvocationIdentity,
    sources: Option<String>,
    derivations: Vec<Derivation>,
    items: &[T],
) -> io::Result<EncodedEntry> {
    let derivations: Vec<Derivation> = derivations
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut body = Vec::new();
    for item in items {
        write_ndjson(&mut body, item)?;
    }
    let key = cache_key_under(&identity, sources.as_deref());
    let meta = Meta {
        key: key.clone(),
        identity: IdentityRecord::from(identity),
        sources: sources.clone(),
        derivations: derivations.clone(),
        items: items.len(),
        digest: items_digest(&body),
    };
    let mut bytes = EntryClass::Cache.header().into_bytes();
    write_ndjson(&mut bytes, &meta)?;
    bytes.extend_from_slice(&body);
    Ok(EncodedEntry {
        entry: OwnedEntry::Cache(CacheEntry {
            key,
            identity,
            sources,
            derivations,
            items: items.len(),
        }),
        bytes,
    })
}

/// Scan `dir` and classify every entry in it. A missing directory is an
/// empty, absent scan. Any failure to enumerate or classify an entry is
/// a coverage error (`KDLITH-CAC003`): no partial scan is ever returned,
/// so nothing acts on an incomplete picture.
pub fn scan_state(dir: &Path) -> Result<CacheScan, Box<DiagnosticRecord>> {
    let shown = dir.display().to_string();
    let meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CacheScan::default()),
        Err(error) => return Err(inaccessible("read", &shown, &error.to_string())),
    };
    if !meta.is_dir() {
        return Err(inaccessible("read", &shown, "not a directory"));
    }
    let read = fs::read_dir(dir).map_err(|e| inaccessible("read", &shown, &e.to_string()))?;
    let mut scan = CacheScan {
        present: true,
        ..CacheScan::default()
    };
    for item in read {
        let item = item.map_err(|e| inaccessible("read", &shown, &e.to_string()))?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| inaccessible("read", &path.display().to_string(), &e.to_string()))?;
        match classify(&path, &name, kind.is_file())? {
            Class::Entry(entry) => scan.entries.push(*entry),
            Class::Corrupt(fault) => scan.corrupt.push((name, fault)),
            Class::Foreign => scan.foreign.push(name),
        }
    }
    scan.entries
        .sort_by(|a, b| (a.class(), a.key()).cmp(&(b.class(), b.key())));
    scan.corrupt.sort_by(|a, b| a.0.cmp(&b.0));
    scan.foreign.sort();
    Ok(scan)
}

/// What one cleaning did: the owned file names it removed, sorted, and
/// one `KDLITH-CAC003` per owned file it could not remove.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct CleanOutcome {
    pub removed: Vec<String>,
    pub failures: Vec<DiagnosticRecord>,
}

impl CleanOutcome {
    /// Whether every owned entry the scan found was removed.
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Remove every owned entry the scan found — intact or corrupt — and
/// nothing else. Foreign entries stay where they are. Every removal is
/// attempted: a failed one is recorded as `KDLITH-CAC003` and the
/// cleaning goes on, so the outcome names every failure, never only
/// the first.
pub fn clean_state(dir: &Path, scan: &CacheScan) -> CleanOutcome {
    let mut outcome = CleanOutcome::default();
    for name in scan.owned() {
        let path = dir.join(&name);
        match fs::remove_file(&path) {
            Ok(()) => outcome.removed.push(name),
            Err(error) => outcome.failures.push(*inaccessible(
                "remove",
                &path.display().to_string(),
                &error.to_string(),
            )),
        }
    }
    outcome
}

/// What one write changed: the entry file names written, and the owned
/// files the new state no longer holds and that were therefore removed.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WriteOutcome {
    pub written: Vec<String>,
    pub removed: Vec<String>,
}

/// Replace the owned entries of `dir` with `entries`: create the
/// directory itself when it does not exist (its parent must), write
/// each entry to a hidden part file and rename it into place, then
/// remove every owned file the new state does not name — corrupt ones
/// included, since they are owned. A scan holding any foreign entry is
/// refused before anything is written: the store never writes into a
/// directory it cannot account for.
pub fn write_state(
    dir: &Path,
    scan: &CacheScan,
    entries: &[EncodedEntry],
) -> Result<WriteOutcome, Box<DiagnosticRecord>> {
    let shown = dir.display().to_string();
    if !scan.foreign.is_empty() {
        return Err(inaccessible(
            "write",
            &shown,
            "it holds entries the cache does not own",
        ));
    }
    if !scan.present {
        fs::create_dir(dir).map_err(|e| inaccessible("create", &shown, &e.to_string()))?;
    }
    let mut outcome = WriteOutcome::default();
    let mut kept = BTreeSet::new();
    for encoded in entries {
        let (class, key) = (encoded.entry.class(), encoded.entry.key());
        let name = class.file_name(key);
        let part = dir.join(class.part_name(key));
        let dest = dir.join(&name);
        let label = dest.display().to_string();
        let staged = fs::File::create(&part).and_then(|mut file| {
            file.write_all(&encoded.bytes)?;
            file.sync_all()
        });
        if let Err(error) = staged.and_then(|()| fs::rename(&part, &dest)) {
            let _ = fs::remove_file(&part);
            return Err(inaccessible("write", &label, &error.to_string()));
        }
        outcome.written.push(name.clone());
        kept.insert(name);
    }
    for name in scan.owned() {
        if kept.contains(&name) {
            continue;
        }
        let path = dir.join(&name);
        fs::remove_file(&path).map_err(|error| {
            inaccessible("remove", &path.display().to_string(), &error.to_string())
        })?;
        outcome.removed.push(name);
    }
    Ok(outcome)
}

/// The classification of one directory entry.
enum Class {
    Entry(Box<OwnedEntry>),
    Corrupt(EntryFault),
    Foreign,
}

/// Classify one entry by name, kind, and header. Links, directories,
/// and special files are foreign and never opened. A file that cannot
/// be read is a coverage error, not a classification.
fn classify(path: &Path, name: &str, regular: bool) -> Result<Class, Box<DiagnosticRecord>> {
    if !regular {
        return Ok(Class::Foreign);
    }
    if let Some(class) = part_class(name) {
        let bytes = read_capped(path)?;
        let magic = class.magic().as_bytes();
        let prefix = bytes.len().min(magic.len());
        return Ok(if bytes[..prefix] == magic[..prefix] {
            Class::Corrupt(EntryFault::Interrupted)
        } else {
            Class::Foreign
        });
    }
    let Some((class, key)) = entry_key(name) else {
        return Ok(Class::Foreign);
    };
    let bytes = read_capped(path)?;
    if !class.marks(&bytes) {
        return Ok(Class::Foreign);
    }
    let decoded = match class {
        EntryClass::Cache => decode(key, &bytes).map(OwnedEntry::Cache),
        EntryClass::Snapshot => {
            decode_snapshot(key, &bytes).map(|(entry, _)| OwnedEntry::Snapshot(entry))
        }
    };
    Ok(match decoded {
        Ok(entry) => Class::Entry(Box::new(entry)),
        Err(fault) => Class::Corrupt(fault),
    })
}

/// Read a file whole, refusing one larger than [`MAX_ENTRY_BYTES`] by
/// returning only the first `MAX_ENTRY_BYTES + 1` bytes; the size check
/// in [`decode`] then rejects it.
fn read_capped(path: &Path) -> Result<Vec<u8>, Box<DiagnosticRecord>> {
    let shown = path.display().to_string();
    let file = fs::File::open(path).map_err(|e| inaccessible("read", &shown, &e.to_string()))?;
    let opened = file
        .metadata()
        .map_err(|e| inaccessible("read", &shown, &e.to_string()))?;
    if !opened.is_file() {
        return Err(inaccessible("read", &shown, "changed while it was scanned"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| inaccessible("read", &shown, &e.to_string()))?;
    Ok(bytes)
}

/// Checked access to one owned entry: header, metadata, key, digest,
/// and item lines, each verified before anything is trusted.
fn decode(key: &str, bytes: &[u8]) -> Result<CacheEntry, EntryFault> {
    let damaged = |reason: String| EntryFault::Damaged(reason);
    let (meta, body) = envelope(EntryClass::Cache, bytes)?;
    let meta: Meta = serde_json::from_str(meta)
        .map_err(|error| damaged(format!("unreadable metadata: {error}")))?;
    let identity = meta.identity.parse().map_err(damaged)?;
    if meta.key != key || cache_key_under(&identity, meta.sources.as_deref()) != key {
        return Err(damaged(
            "its key does not match its file name and identity".to_owned(),
        ));
    }
    if items_digest(body.as_bytes()) != meta.digest {
        return Err(damaged("its items do not match their digest".to_owned()));
    }
    let mut items = 0;
    for line in body.split_terminator('\n') {
        serde_json::from_str::<serde_json::Value>(line)
            .map_err(|error| damaged(format!("unreadable item line: {error}")))?;
        items += 1;
    }
    if items != meta.items || !(body.is_empty() || body.ends_with('\n')) {
        return Err(damaged(
            "its item count does not match its item lines".to_owned(),
        ));
    }
    let mut sorted = meta.derivations.clone();
    sorted.sort();
    sorted.dedup();
    if sorted != meta.derivations {
        return Err(damaged(
            "its derivations are not in canonical order".to_owned(),
        ));
    }
    Ok(CacheEntry {
        key: key.to_owned(),
        identity,
        sources: meta.sources,
        derivations: meta.derivations,
        items,
    })
}

/// Checked access to one snapshot entry: header, metadata, key, body
/// digest, then every record line — its validation status, its
/// snapshot id, its slot count, and its slot names against the layout
/// digest — each verified before anything is trusted.
fn decode_snapshot(
    key: &str,
    bytes: &[u8],
) -> Result<(SnapshotEntry, Vec<SnapshotRecord>), EntryFault> {
    let damaged = |reason: String| EntryFault::Damaged(reason);
    let (meta, body) = envelope(EntryClass::Snapshot, bytes)?;
    let meta: SnapshotMeta = serde_json::from_str(meta)
        .map_err(|error| damaged(format!("unreadable metadata: {error}")))?;
    let recomputed = match (undigest(&meta.revision), undigest(&meta.profiles)) {
        (Ok(revision), Ok(profiles)) => snapshot_key(&meta.entry, revision, profiles),
        (Err(reason), _) | (_, Err(reason)) => return Err(damaged(reason)),
    };
    if meta.key != key || recomputed != key {
        return Err(damaged(
            "its key does not match its file name and identity".to_owned(),
        ));
    }
    undigest(&meta.schema).map_err(damaged)?;
    if snapshot_digest(body.as_bytes()) != meta.digest {
        return Err(damaged("its payload does not match its digest".to_owned()));
    }
    if !(body.is_empty() || body.ends_with('\n')) {
        return Err(damaged("its last record line is unterminated".to_owned()));
    }
    let mut records = Vec::new();
    for (at, line) in body.split_terminator('\n').enumerate() {
        let line: RecordLine = serde_json::from_str(line)
            .map_err(|error| damaged(format!("unreadable record line: {error}")))?;
        let (record, names) = line.into_record().map_err(damaged)?;
        if record.snapshot != format!("{key}/{at}") {
            return Err(damaged(format!(
                "record {at} names snapshot {:?}",
                record.snapshot
            )));
        }
        if names.len() != meta.slots
            || slot_layout_digest(names.iter().map(String::as_str)) != meta.layout
        {
            return Err(damaged(format!(
                "record {at}'s slots do not match the layout"
            )));
        }
        records.push(record);
    }
    if records.len() != meta.records {
        return Err(damaged(
            "its record count does not match its record lines".to_owned(),
        ));
    }
    Ok((
        SnapshotEntry {
            key: key.to_owned(),
            meta,
        },
        records,
    ))
}

/// The metadata line and body of an owned entry of `class`, once its
/// size, encoding, and ownership header are checked. A header naming
/// another format or canonicalization is a version outcome, never read
/// further.
fn envelope(class: EntryClass, bytes: &[u8]) -> Result<(&str, &str), EntryFault> {
    let damaged = |reason: &str| EntryFault::Damaged(reason.to_owned());
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(EntryFault::Damaged(format!(
            "larger than {MAX_ENTRY_BYTES} bytes"
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| damaged("not UTF-8"))?;
    let (head, rest) = text
        .split_once('\n')
        .ok_or_else(|| damaged("no metadata line"))?;
    let current = class.header();
    if format!("{head}\n") != current {
        return Err(EntryFault::Version {
            header: head.to_owned(),
            current: current.trim_end().to_owned(),
        });
    }
    rest.split_once('\n')
        .ok_or_else(|| damaged("no metadata line"))
}

/// The metadata line of an entry.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    key: String,
    identity: IdentityRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sources: Option<String>,
    derivations: Vec<Derivation>,
    items: usize,
    digest: String,
}

/// An invocation identity as an entry spells it: each fingerprint as a
/// stamped digest, the policy by name.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityRecord {
    revision: String,
    schema: String,
    context: String,
    profiles: String,
    resolved: String,
    policy: Repeated,
}

impl From<InvocationIdentity> for IdentityRecord {
    fn from(identity: InvocationIdentity) -> Self {
        Self {
            revision: digest(identity.revision),
            schema: digest(identity.schema),
            context: digest(identity.context),
            profiles: digest(identity.profiles),
            resolved: digest(identity.resolved),
            policy: identity.policy,
        }
    }
}

impl IdentityRecord {
    fn parse(&self) -> Result<InvocationIdentity, String> {
        Ok(InvocationIdentity {
            revision: undigest(&self.revision)?,
            schema: undigest(&self.schema)?,
            context: undigest(&self.context)?,
            profiles: undigest(&self.profiles)?,
            resolved: undigest(&self.resolved)?,
            policy: self.policy,
        })
    }
}

/// The digest of an entry's item lines, under its own hash domain.
fn items_digest(body: &[u8]) -> String {
    digest(stable_hash_step(stable_hash(b"kdlith-cache-items"), body))
}

/// The digest of a snapshot entry's payload, under its own hash domain.
fn snapshot_digest(body: &[u8]) -> String {
    digest(stable_hash_step(stable_hash(b"kdlith-snapshot-body"), body))
}

/// The class and key of an entry file name, when the name is one.
fn entry_key(name: &str) -> Option<(EntryClass, &str)> {
    CLASSES.iter().find_map(|(class, suffix, _)| {
        name.strip_suffix(suffix)
            .filter(|key| is_key(key))
            .map(|key| (*class, key))
    })
}

/// The class of a part file name, when the name is one.
fn part_class(name: &str) -> Option<EntryClass> {
    let rest = name.strip_prefix('.')?;
    CLASSES.iter().find_map(|(class, suffix, _)| {
        rest.strip_suffix("-part")
            .and_then(|entry| entry.strip_suffix(suffix))
            .filter(|key| is_key(key))
            .map(|_| *class)
    })
}

fn is_key(key: &str) -> bool {
    key.len() == 16
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn digest(hash: u64) -> String {
    format!("fnv1a64:{hash:016x}")
}

fn undigest(text: &str) -> Result<u64, String> {
    text.strip_prefix("fnv1a64:")
        .filter(|hex| is_key(hex))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .ok_or_else(|| format!("{text:?} is not an fnv1a64 digest"))
}

fn policy_name(policy: Repeated) -> &'static str {
    match policy {
        Repeated::Coalesce => "coalesce",
        Repeated::Instantiate => "instantiate",
        _ => "reject",
    }
}

/// A `KDLITH-CAC003` record.
fn inaccessible(action: &str, path: &str, reason: &str) -> Box<DiagnosticRecord> {
    Box::new(coded(INACCESSIBLE, &[action, path, reason]))
}

/// A record for one of the store's registered codes.
fn coded(code: &str, values: &[&str]) -> DiagnosticRecord {
    let record =
        DiagnosticRecord::from_code(code).expect("the cache codes are registered and live");
    let record = match lookup(code) {
        Some(definition) => record.with_message(render(definition.message_template, values)),
        None => record,
    };
    record.with_origin("cache")
}
