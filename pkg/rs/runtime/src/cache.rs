//! The persistent derived-state store behind `kdlith cache`: one
//! versioned, owned entry file per cached bound result, in a state
//! directory every operation names explicitly.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};
use kdlith_model::Repeated;
use serde::{Deserialize, Serialize};

use crate::context::{stable_hash, stable_hash_step};
use crate::export::write_ndjson;
use crate::imports::InvocationIdentity;

/// An entry is owned when it is a regular file named
/// `<key>.kdlith-cache` whose first line is the ownership header;
/// anything else in the directory is foreign — reported, never read
/// further, never removed.
const SUFFIX: &str = ".kdlith-cache";

/// The entry format version this build reads and writes.
pub const CACHE_FORMAT: u32 = 2;
/// The item canonicalization version: how bound items are serialized
/// into an entry. A change to the serialized item shape takes a new
/// number, so an entry written under another one is never misread.
pub const CACHE_CANONICALIZATION: u32 = 1;
/// The largest entry file the store reads; a larger one is corrupt.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

const FOREIGN: &str = "KDLITH-CAC001";
const CORRUPT: &str = "KDLITH-CAC002";
const INACCESSIBLE: &str = "KDLITH-CAC003";
const MAGIC: &str = "kdlith-cache ";
const PART_SUFFIX: &str = ".kdlith-cache-part";

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
    pub derivations: Vec<Derivation>,
    pub items: usize,
}

impl fmt::Display for CacheEntry {
    /// The entry line, then one indented line per derivation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = &self.identity;
        write!(
            f,
            "{} revision={} schema={} context={} profiles={} policy={} items={}",
            self.key,
            digest(id.revision),
            digest(id.schema),
            digest(id.context),
            digest(id.profiles),
            policy_name(id.policy),
            self.items
        )?;
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

/// Why an owned file is not an intact entry. A version mismatch is its
/// own outcome, distinct from damage, so a newer or older store's entry
/// is never misread as this one's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryFault {
    /// The ownership header names another format or canonicalization.
    Version { header: String },
    /// A write was interrupted and left its hidden part file.
    Interrupted,
    /// The entry fails a structural check.
    Damaged(String),
}

impl fmt::Display for EntryFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryFault::Version { header } => write!(
                f,
                "written as {header:?}, not this version's {:?}",
                header_line().trim_end()
            ),
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
    /// Intact entries, sorted by key.
    pub entries: Vec<CacheEntry>,
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
            .map(|entry| file_name(&entry.key))
            .chain(self.corrupt.iter().map(|(name, _)| name.clone()))
            .collect()
    }
}

/// One entry ready to write: its key and its encoded bytes.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct EncodedEntry {
    pub entry: CacheEntry,
    bytes: Vec<u8>,
}

/// The key of an invocation identity: a stable FNV-1a hash of its five
/// components, as 16 lowercase hex digits.
pub fn cache_key(identity: &InvocationIdentity) -> String {
    let mut hash = stable_hash(b"kdlith-cache-key");
    hash = stable_hash_step(hash, &identity.revision.to_le_bytes());
    hash = stable_hash_step(hash, &identity.schema.to_le_bytes());
    hash = stable_hash_step(hash, &identity.context.to_le_bytes());
    hash = stable_hash_step(hash, &identity.profiles.to_le_bytes());
    hash = stable_hash_step(hash, policy_name(identity.policy).as_bytes());
    format!("{hash:016x}")
}

/// Encode one entry: the ownership header, one metadata line, then one
/// NDJSON line per bound item. Derivations are sorted and deduplicated,
/// so equal inputs encode to equal bytes.
pub fn encode_entry<T: Serialize>(
    identity: InvocationIdentity,
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
    let key = cache_key(&identity);
    let meta = Meta {
        key: key.clone(),
        identity: IdentityRecord::from(identity),
        derivations: derivations.clone(),
        items: items.len(),
        digest: items_digest(&body),
    };
    let mut bytes = header_line().into_bytes();
    write_ndjson(&mut bytes, &meta)?;
    bytes.extend_from_slice(&body);
    Ok(EncodedEntry {
        entry: CacheEntry {
            key,
            identity,
            derivations,
            items: items.len(),
        },
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
            Class::Entry(entry) => scan.entries.push(entry),
            Class::Corrupt(fault) => scan.corrupt.push((name, fault)),
            Class::Foreign => scan.foreign.push(name),
        }
    }
    scan.entries.sort_by(|a, b| a.key.cmp(&b.key));
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

/// What one write changed: the keys written, and the owned files the
/// new state no longer holds and that were therefore removed.
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
        let name = file_name(&encoded.entry.key);
        let part = dir.join(format!(".{}{PART_SUFFIX}", encoded.entry.key));
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
        outcome.written.push(encoded.entry.key.clone());
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
    Entry(CacheEntry),
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
    let part = part_key(name);
    let key = match (entry_key(name), part) {
        (Some(key), _) => key,
        (None, Some(_)) => {
            let bytes = read_capped(path)?;
            let prefix = bytes.len().min(MAGIC.len());
            return Ok(if bytes[..prefix] == MAGIC.as_bytes()[..prefix] {
                Class::Corrupt(EntryFault::Interrupted)
            } else {
                Class::Foreign
            });
        }
        (None, None) => return Ok(Class::Foreign),
    };
    let bytes = read_capped(path)?;
    if !bytes.starts_with(MAGIC.as_bytes()) {
        return Ok(Class::Foreign);
    }
    Ok(match decode(key, &bytes) {
        Ok(entry) => Class::Entry(entry),
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
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(damaged(format!("larger than {MAX_ENTRY_BYTES} bytes")));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| damaged("not UTF-8".to_owned()))?;
    let (head, rest) = text
        .split_once('\n')
        .ok_or_else(|| damaged("no metadata line".to_owned()))?;
    if format!("{head}\n") != header_line() {
        return Err(EntryFault::Version {
            header: head.to_owned(),
        });
    }
    let (meta, body) = rest
        .split_once('\n')
        .ok_or_else(|| damaged("no metadata line".to_owned()))?;
    let meta: Meta = serde_json::from_str(meta)
        .map_err(|error| damaged(format!("unreadable metadata: {error}")))?;
    let identity = meta.identity.parse().map_err(damaged)?;
    if meta.key != key || cache_key(&identity) != key {
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
        derivations: meta.derivations,
        items,
    })
}

/// The metadata line of an entry.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    key: String,
    identity: IdentityRecord,
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
    policy: Repeated,
}

impl From<InvocationIdentity> for IdentityRecord {
    fn from(identity: InvocationIdentity) -> Self {
        Self {
            revision: digest(identity.revision),
            schema: digest(identity.schema),
            context: digest(identity.context),
            profiles: digest(identity.profiles),
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
            policy: self.policy,
        })
    }
}

/// The ownership header: the marker, the format, and the
/// canonicalization the items were written under.
fn header_line() -> String {
    format!("{MAGIC}format={CACHE_FORMAT} canonicalization={CACHE_CANONICALIZATION}\n")
}

/// The digest of an entry's item lines, under its own hash domain.
fn items_digest(body: &[u8]) -> String {
    digest(stable_hash_step(stable_hash(b"kdlith-cache-items"), body))
}

fn file_name(key: &str) -> String {
    format!("{key}{SUFFIX}")
}

/// The key of an entry file name, when the name is one.
fn entry_key(name: &str) -> Option<&str> {
    name.strip_suffix(SUFFIX).filter(|key| is_key(key))
}

/// The key of a part file name, when the name is one.
fn part_key(name: &str) -> Option<&str> {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(PART_SUFFIX))
        .filter(|key| is_key(key))
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
