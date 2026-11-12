//! The data-path reader: binds data documents through a compiled
//! definition and executes their imports as invocations, under the
//! identity and cache rules the runtime records.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{ImportRequest, Item, SourceDocument, SourceId};
use kdlith_runtime::{
    Admission, Context, ContributionRecord, Derivation, Handlers, ImportFailure, InvocationId,
    InvocationIdentity, Invocations, MAX_INVOCATIONS, SlotId, SlotValue, snapshot_key, stable_hash,
    stable_hash_step,
};

use crate::bind::{BindingSession, dispatch_scope};
use crate::definition::{Definition, DefinitionKind, ProfileSelection};
use crate::diag::{UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA, record};
use crate::digest::{bytes_hex, lines_digest};
use crate::documents::{import_failure, read_text};
use crate::list::record_identity;
use crate::locations::{DeclaredLocations, Home, Importer, Place, SourceTree, Sources, Target};
use crate::names::Sym;
use crate::resolution::{SharedContributionResolver, failure_record, resolve};
use crate::snapshot_store::{SnapshotLoad, load_snapshot};
use crate::snapshots::{
    ContextSnapshot, SnapshotError, ValidationStatus as SnapshotValidation, snapshot_fingerprint,
};
use crate::typed::ReadError;
use crate::types::OutputPolicy;
use crate::value_resolver::SharedResolver;

const ORIGIN: &str = "data-document";

/// Takes every slot's value in slot order: each document's entry context
/// just before its text, and the context each record or declaration
/// bound under just before the item.
pub(crate) type Entering<'e> = &'e mut dyn FnMut(&[Option<SlotValue>]);

/// Takes the profile selection of the entry document a document item
/// belongs to, just before the item: the selection every document the
/// entry's imports reach binds under.
pub(crate) type Profiling<'p> = &'p mut dyn FnMut(&ProfileSelection);

/// What one reading supplies beyond its definition: the profile, or the
/// profile set, the invocation names, the project root, and the named
/// sources import locations may address as `$name/`. A source that is
/// malformed, repeated, or does not resolve fails the reading before
/// any document is read (`KDLITH-LOC005`).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ReadOptions {
    profile: Option<String>,
    profiles: Vec<String>,
    sources: Vec<(String, SourceTree)>,
    root: Option<PathBuf>,
    manifest: Option<PathBuf>,
    overlay: Option<PathBuf>,
    overrides: Vec<(String, PathBuf)>,
    store: Option<PathBuf>,
    output_policy: Option<OutputPolicy>,
    max_invocations: Option<usize>,
    threads: Option<usize>,
    demand: Option<Vec<String>>,
}

impl ReadOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Name the profile the invocation binds. Without one, each entry
    /// document's profile is selected from its source path by the
    /// schema's declared match rules, else its default profile; a
    /// definition declaring profiles that selects none fails closed
    /// (`KDLITH-SCH011`).
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    /// Name the profile set the invocation admits, in listed order.
    /// Each entry document's selection resolves among the set's members
    /// (see [`Definition::select_profile`]): the members whose match
    /// rules its source path satisfies, else the member marked default,
    /// else nothing. A member the definition does not declare, or an
    /// entry no member selects, fails closed (`KDLITH-SCH011`). An
    /// explicitly named profile takes precedence over the set.
    #[must_use]
    pub fn with_profiles<I, S>(mut self, profiles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.profiles = profiles.into_iter().map(Into::into).collect();
        self
    }

    /// Apply one explicit output policy to every sensitive field the
    /// reading binds; without one, each sensitivity keeps its
    /// fail-closed default (a secret is redacted).
    #[must_use]
    pub fn with_output_policy(mut self, policy: OutputPolicy) -> Self {
        self.output_policy = Some(policy);
        self
    }

    /// Admit at most `max` invocations in one reading — the entry
    /// documents and every import instantiation together — in place of
    /// the default [`MAX_INVOCATIONS`]. The invocation past the bound
    /// fails closed (`KDLITH-SRC003`).
    #[must_use]
    pub fn with_max_invocations(mut self, max: usize) -> Self {
        self.max_invocations = Some(max);
        self
    }

    /// Read only what the record types `types` name can come from. A
    /// node whose item is of no demanded type, and inside which nothing
    /// demanded can be written, is skipped where it opens: its text is
    /// passed over, nothing in it is decoded or validated, and it yields
    /// no item. Imports are still followed, a node whose effects reach
    /// its siblings still binds, and so does every node on the way down
    /// to a demanded one, so each demanded item binds under the context
    /// a full reading binds it under.
    ///
    /// A demand trusts the documents for everything it skips: a reading
    /// that must validate a tree reads it without one. Such a reading
    /// resolves no contributions, and linking it sees only the
    /// declarations it bound. A name the definition declares no record
    /// type for fails the reading before any document is read
    /// ([`ReadError::UnknownType`]).
    #[must_use]
    pub fn with_demand<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.demand = Some(types.into_iter().map(Into::into).collect());
        self
    }

    /// Bind on `threads` threads in place of one. The documents one
    /// import request fans out to are read and bound ahead of their
    /// turn on the other threads, since no import's effects reach its
    /// importer or its siblings, and their items are given in the order
    /// a single thread gives them: the reading is the same whatever the
    /// count. A reading with context handlers or a value resolver
    /// installed binds on one thread, so they are called in document
    /// order.
    ///
    /// Each item is made on one thread and released on another, so the
    /// threads pay off only under an allocator that scales across
    /// threads, such as jemalloc or mimalloc; under one that guards
    /// each thread's memory with a lock, a second thread costs more
    /// than it binds.
    #[must_use]
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads.max(1));
        self
    }

    /// Register one named source: a location written `$name/PATH`
    /// resolves PATH from the source's root wherever the importing
    /// document lives, and every location a document of that source
    /// writes stays inside it (`KDLITH-LOC002`). A directory-backed
    /// source is read from disk; an embedded one serves its files from
    /// memory, so a document held in memory may address it too.
    #[must_use]
    pub fn with_source(mut self, name: impl Into<String>, tree: SourceTree) -> Self {
        self.sources.push((name.into(), tree));
        self
    }

    /// Name the project root in place of the current directory: the
    /// root of the local source, which every unprefixed location of a
    /// document under it may reach and none may leave, and the root
    /// profile match rules read entry paths relative to (see
    /// [`crate::match_path`]).
    #[must_use]
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// Read the sources manifest at `path`, which declares sources by
    /// name: a directory (`source NAME dir=DIR`) or an origin whose tree
    /// the store holds, optionally pinned by its digest; it may also
    /// override a declared source. kdlith never fetches an origin. A
    /// manifest name the host also registers is `KDLITH-LOC005`.
    #[must_use]
    pub fn with_sources_manifest(mut self, path: impl Into<PathBuf>) -> Self {
        self.manifest = Some(path.into());
        self
    }

    /// Read the overrides file at `path`: overrides only, a layer above
    /// the manifest's own.
    #[must_use]
    pub fn with_overrides_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.overlay = Some(path.into());
        self
    }

    /// Override the declared source `name` with the directory `dir`, the
    /// most local layer. Every override is a `KDLITH-LOC009` warning.
    #[must_use]
    pub fn with_override(mut self, name: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        self.overrides.push((name.into(), dir.into()));
        self
    }

    /// Name the store origin sources resolve from:
    /// `<store>/sha256/<hex>/` for a pinned origin, and
    /// `<store>/origin/<key>/` for an unpinned one.
    #[must_use]
    pub fn with_store(mut self, store: impl Into<PathBuf>) -> Self {
        self.store = Some(store.into());
        self
    }

    /// Whether these options demand some record types alone.
    pub(crate) fn demands(&self) -> bool {
        self.demand.is_some()
    }

    /// What the reading's source set is made from.
    pub(crate) fn source_inputs(&self) -> crate::sources::SourceInputs<'_> {
        crate::sources::SourceInputs {
            root: self.root.as_deref(),
            registered: &self.sources,
            manifest: self.manifest.as_deref(),
            overlay: self.overlay.as_deref(),
            overrides: &self.overrides,
            store: self.store.as_deref(),
        }
    }

    /// The source set these options name, as a reading admits it; `None`
    /// when it does not admit, which the reading itself reports.
    pub(crate) fn asset_set(&self) -> Option<crate::locations::AssetSet> {
        Sources::admit(&self.source_inputs(), ORIGIN)
            .ok()
            .map(|(sources, _)| crate::locations::AssetSet(Rc::new(sources)))
    }

    /// The profile these options name, and the set they admit, as they
    /// spell them: what a typed reader asked for provenance validates
    /// and surfaces.
    pub(crate) fn profile_inputs(&self) -> (Option<&str>, &[String]) {
        (self.profile.as_deref(), &self.profiles)
    }

    /// Select `def`'s profiles for the entry the reading shows as
    /// `shown`, under these options.
    fn selection(&self, def: &Definition, shown: &str) -> ProfileSelection {
        def.select_profile_at(
            self.profile.as_deref(),
            &self.profiles,
            shown,
            self.root.as_deref(),
        )
    }
}

/// A data document held in memory instead of read from disk: the path
/// its provenance shows, and its text. The revision digest and
/// invocation identity derive from the text exactly as a file
/// reading's do, so a `Source` holding a file's bytes and that file
/// read as an entry produce one reading. A document held in memory
/// belongs to no source: only its `$name/` locations resolve, and any
/// other import inside it fails closed (`KDLITH-LOC001`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Source {
    /// The path as the reading shows the document.
    pub shown: String,
    /// The document's text, as a file reading would read it.
    pub text: String,
}

impl Source {
    pub fn new(shown: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            shown: shown.into(),
            text: text.into(),
        }
    }
}

/// An entry as the reading locates it: a document on disk, or one held
/// in memory under the spelling its provenance shows.
pub(crate) enum EntrySource<'a> {
    Path(&'a Path),
    Memory { shown: &'a str, text: &'a str },
}

/// One bound result a reading cached: the invocation identity it is
/// keyed by, where it was derived, and its items under one canonical
/// source identity, so equal semantics give equal items wherever the
/// invocation sat.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BoundResult {
    pub identity: InvocationIdentity,
    /// Every place the reading bound or replayed this result: the
    /// entrypoint, the document, and its importer, as the reading
    /// showed them, in reading order.
    pub derivations: Vec<Derivation>,
    /// The result's items, shared with the reading's own replay cache.
    pub items: Rc<Vec<Item>>,
    /// The source-set fingerprint of the reading that bound it: the
    /// named sources the reading used, each with the identity of the
    /// tree it read. `None` when it used none.
    pub sources: Option<String>,
}

/// The context an entry document's root invocation left once its
/// document was read: every slot's value in slot-id order, `None` where
/// the slot is unset. Imported documents run under projections of their
/// importer's context and leave nothing here.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct EntryContext {
    /// The entry document's path as the reading showed it.
    pub entry: String,
    /// The invocation's entry documents as it spelled them, up to and
    /// including this one: the lineage its snapshots are restored under.
    pub lineage: Vec<String>,
    pub values: Vec<Option<SlotValue>>,
    /// The entry document's source revision.
    pub revision: u64,
    /// The fingerprint of the profile selection it was read under.
    pub profiles: u64,
    /// Whether any binding of its reading — its own or an import's —
    /// consulted the value resolver: such a reading's contexts may hold
    /// a resolved secret and are never persisted.
    pub consulted_resolver: bool,
    /// Every record its reading bound or replayed, in stream order, each
    /// with the context it bound under.
    pub records: Vec<RecordContext>,
    /// Each named source the reading had used by the time this entry's
    /// root completed, with the identity of the tree it read: what a
    /// restore of its records checks.
    pub sources: BTreeMap<String, String>,
}

/// One record a reading bound, with the context it bound under: what a
/// snapshot index entry records so the record can later be resolved by
/// restoring that context and binding its span.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RecordContext {
    /// The record's `id` field, when it has one.
    pub identity: Option<String>,
    pub ty: String,
    /// The revision of the source the record was bound from.
    pub revision: u64,
    /// The invocation, and so the source id, the record was bound in.
    pub source: u32,
    /// The record node's index in that source.
    pub node: u32,
    /// The byte span the record reports: its node's name.
    pub span: (u32, u32),
    pub validation: SnapshotValidation,
    /// Every slot's value in slot-id order as the record bound.
    pub values: Vec<Option<SlotValue>>,
}

/// What resolving one record from the snapshot store came to.
#[derive(Debug)]
#[non_exhaustive]
pub enum Restored {
    /// No snapshot this invocation can use: the record's document
    /// changed since the scan, or the store indexes no such record. The
    /// caller replays.
    Miss,
    /// The record's span was bound under its restored context; whether
    /// any error diagnostic was among the items.
    Bound(bool),
    /// The store holds an entry under the key that cannot be read
    /// (`KDLITH-CAC002`, `KDLITH-CAC003`).
    Unusable(Box<DiagnosticRecord>),
    /// The snapshot cannot restore under this definition or invocation
    /// (`KDLITH-CTX004`), with the snapshot's id; never a best-effort
    /// restore.
    Mismatch(Box<(SnapshotError, String)>),
}

/// A reader of data documents over one compiled definition: the entry
/// documents, and the documents they import, as one item stream.
pub struct DataReader<'d> {
    def: &'d Definition,
    /// The context handlers the reading processor installed, carried
    /// into every invocation: a handler effect inside an imported
    /// document executes against that invocation's own projected
    /// context, never the caller's.
    handlers: Handlers,
    /// The value resolver the consuming program registered, carried
    /// into every invocation: an absent field whose declared fallback is
    /// computed resolves through it.
    resolver: Option<SharedResolver>,
    /// The contribution resolver the processor registered: the reading
    /// routes every preserved contribution record to it under the
    /// schema's declared policy.
    contributions: Option<SharedContributionResolver>,
}

impl<'d> DataReader<'d> {
    pub fn new(def: &'d Definition) -> Self {
        Self::with_handlers(def, Handlers::new())
    }

    /// A reader whose context handlers are the ones the processor
    /// installed; the definition must have been compiled with the same
    /// names, or each missing handler fails at its effect site.
    #[must_use]
    pub fn with_handlers(def: &'d Definition, handlers: Handlers) -> Self {
        Self {
            def,
            handlers,
            resolver: None,
            contributions: None,
        }
    }

    /// The reader resolving every absent computed-default field through
    /// `resolver`, the consuming program's static value-resolution hook.
    /// A value it declines fails its node loudly (`KDLITH-BND009`).
    #[must_use]
    pub fn with_resolver(mut self, resolver: SharedResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// The reader routing the reading's contribution records to
    /// `resolver` once its documents are read, when the schema declares
    /// a resolution policy. Each target it settles follows the
    /// documents' items as a resolution item; a refusal fails the
    /// reading (`KDLITH-DLC010`).
    #[must_use]
    pub fn with_contribution_resolver(mut self, resolver: SharedContributionResolver) -> Self {
        self.contributions = Some(resolver);
        self
    }

    /// The reader with its context handlers replaced by `handlers`.
    pub(crate) fn replace_handlers(mut self, handlers: Handlers) -> Self {
        self.handlers = handlers;
        self
    }

    /// The definition the reader binds under.
    pub fn definition(&self) -> &'d Definition {
        self.def
    }

    /// Read as [`DataReader::read`] does, or as [`DataReader::read_local`]
    /// when `follow` is false, also giving each document's text to
    /// `texts` immediately before the document item it belongs to, so a
    /// consumer pairs them in stream order without reading a file again;
    /// with `entered`, also giving it, at the same point, every slot's
    /// value in slot order as the document's invocation entered it, and
    /// immediately before each record or declaration, the values it
    /// bound under; with `profiling`, also giving it, at the same point,
    /// the profile selection of the entry document the item belongs to.
    /// An entry may be a document held in memory.
    pub(crate) fn read_documents(
        &self,
        entries: &[EntrySource<'_>],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        sides: Sides<'_, '_, '_>,
        follow: bool,
    ) -> Result<bool, ReadError> {
        self.read_inner(entries, options, sink, sides, Mode::plain(follow))
            .map(|outcome| outcome.failed)
    }

    /// Read every entry document and the documents they import, giving
    /// each item to `sink` as it is read. Each document's items follow a
    /// document item naming its source and path. `Ok(true)` when any
    /// error diagnostic was among the items.
    pub fn read(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<bool, ReadError> {
        self.read_scoped(entries, options, sink, Mode::plain(true))
            .map(|outcome| outcome.failed)
    }

    /// Read as [`DataReader::read`] does, over documents held in memory
    /// instead of read from disk: each `Source`'s text binds as its
    /// shown path's document would, with the same revision digest and
    /// provenance. An in-memory document belongs to no source: only its
    /// `$name/` locations resolve, any other fails closed
    /// (`KDLITH-LOC001`).
    pub fn read_sources(
        &self,
        sources: &[Source],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<bool, ReadError> {
        let entries: Vec<EntrySource> = sources
            .iter()
            .map(|source| EntrySource::Memory {
                shown: &source.shown,
                text: &source.text,
            })
            .collect();
        self.read_inner(&entries, options, sink, Sides::none(), Mode::plain(true))
            .map(|outcome| outcome.failed)
    }

    /// Read as [`DataReader::read`] does, and also return every bound
    /// result the reading cached, in the order each was first bound.
    pub fn read_results(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<(bool, Vec<BoundResult>), ReadError> {
        let mode = Mode {
            results: true,
            ..Mode::plain(true)
        };
        self.read_scoped(entries, options, sink, mode)
            .map(|outcome| (outcome.failed, outcome.results))
    }

    /// Read as [`DataReader::read`] does, and also return the context
    /// each entry document's root invocation left, in entry order. An
    /// entry whose document could not be read has none. The contexts
    /// carry no records: only a reading made for the snapshot store
    /// keeps the context every record bound under.
    pub fn read_entry_contexts(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<(bool, Vec<EntryContext>), ReadError> {
        let mode = Mode {
            contexts: true,
            ..Mode::plain(true)
        };
        self.read_scoped(entries, options, sink, mode)
            .map(|outcome| (outcome.failed, outcome.contexts))
    }

    /// Read as [`DataReader::read_results`] does, and also return the
    /// context each entry document's root invocation left.
    pub(crate) fn read_state(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<(bool, Vec<BoundResult>, Vec<EntryContext>), ReadError> {
        let mode = Mode {
            results: true,
            contexts: true,
            records: true,
            ..Mode::plain(true)
        };
        self.read_scoped(entries, options, sink, mode)
            .map(|outcome| (outcome.failed, outcome.results, outcome.contexts))
    }

    /// A checkpoint of each record's context under this reader's
    /// definition, per entry: snapshot `<key>/<position>`, where the key
    /// hashes the entry as its invocation spelled it, its revision, and
    /// its profile selection. The lineage is the invocation's entry
    /// documents as spelled, up to and including this one: entries bound
    /// before it number the invocations after them, so only an
    /// invocation spelling the same chain reproduces its records. An entry whose reading consulted the value
    /// resolver gets none: its contexts may hold a resolved secret.
    pub fn record_snapshots(
        &self,
        contexts: Vec<EntryContext>,
    ) -> Vec<(EntryContext, Option<Vec<ContextSnapshot>>)> {
        let layout = &self.def.plan.context.layout;
        let fingerprint = snapshot_fingerprint(self.def);
        contexts
            .into_iter()
            .map(|entry| {
                if entry.consulted_resolver {
                    return (entry, None);
                }
                let key = snapshot_key(&entry.entry, entry.revision, entry.profiles);
                let lineage = entry.lineage.clone();
                // The values were held by a context over this very
                // layout, so rebuilding the context cannot mismatch it.
                let snapshots = entry
                    .records
                    .iter()
                    .enumerate()
                    .filter_map(|(at, record)| {
                        let context = Context::from_values(layout.clone(), record.values.clone());
                        let id = format!("{key}/{at}");
                        let fingerprint = fingerprint.clone();
                        context.ok().map(|context| {
                            ContextSnapshot::checkpoint(id, &context, fingerprint, lineage.clone())
                        })
                    })
                    .collect();
                (entry, Some(snapshots))
            })
            .collect()
    }

    /// Resolve the record `identity` of the last entry document from the
    /// snapshot store `state` instead of replaying that document: restore
    /// the context the record bound under at the scan, then bind the
    /// record's own span under it, at the node index it had. Nothing else
    /// of the record's document binds, and no context crosses between
    /// documents. The snapshot is keyed by the record's document as
    /// spelled, its revision, and its profile selection; the restore
    /// checks the definition's fingerprint and the invocation's entry
    /// chain. The entries before the record's document still bind, with
    /// only their diagnostics given, because they number the record's
    /// invocation, and so its source id.
    pub fn read_restored(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        state: &Path,
        identity: &str,
        sink: &mut dyn FnMut(Item),
    ) -> Result<Restored, ReadError> {
        let def = self.def;
        let Some((document, earlier)) = entries.split_last() else {
            return Ok(Restored::Miss);
        };
        // A reading that rejects its sources reports it; so does replay.
        let Ok((sources, _)) = Sources::admit(&options.source_inputs(), ORIGIN) else {
            return Ok(Restored::Miss);
        };
        let chain: Vec<String> = entries.iter().map(|e| e.display().to_string()).collect();
        let shown = document.display().to_string();
        let Some((text, key, selection)) = self.scanned(document, &shown, options) else {
            return Ok(Restored::Miss);
        };
        let (record, snapshot) = match load_snapshot(state, &key, identity) {
            Ok(SnapshotLoad::Hit(hit)) => {
                let (entry, record, snapshot) = *hit;
                // A snapshot taken over other trees of a source than this
                // reading's is a miss, never a restore of stale context.
                let same = entry.meta.sources.iter().all(|(name, held)| {
                    sources.identity_of(name, ORIGIN).as_deref() == Some(held.as_str())
                });
                if !same {
                    return Ok(Restored::Miss);
                }
                (record, snapshot)
            }
            Ok(SnapshotLoad::Unusable(record)) | Err(record) => {
                return Ok(Restored::Unusable(record));
            }
            Ok(_) => return Ok(Restored::Miss),
        };
        let fingerprint = snapshot_fingerprint(def);
        let context = match snapshot.restore(&def.plan.context.layout, &fingerprint, &chain) {
            Ok(context) => context,
            Err(error) => {
                let id = snapshot.id().to_owned();
                return Ok(Restored::Mismatch(Box::new((error, id))));
            }
        };
        let revision = revision_digest(stable_hash(text.as_bytes()));
        // A record an import reached was bound from another document's
        // text, which this invocation does not locate.
        if record.digest != revision {
            return Ok(Restored::Miss);
        }
        // The earlier entries bind as in replay: their import trees, which
        // may have grown since the scan, number the record's invocation.
        let mut tap = |item: Item| {
            if let Item::Diagnostic(_) = &item {
                sink(item);
            }
        };
        let earlier: Vec<EntrySource> = earlier
            .iter()
            .map(|entry| EntrySource::Path(entry))
            .collect();
        let mode = Mode {
            resolving: false,
            ..Mode::plain(true)
        };
        let before = self.read_inner(&earlier, options, &mut tap, Sides::none(), mode)?;
        let source = SourceId(u32::try_from(before.entered).unwrap_or(u32::MAX));
        let Some((scope, enclosing)) = dispatch_scope(def, &text, record.node) else {
            return Ok(Restored::Miss);
        };
        sink(Item::Document(
            SourceDocument::new(source, shown.clone()).with_digest(&revision),
        ));
        let mut session = BindingSession::restored(
            def,
            (source, &shown, &text),
            context,
            &selection,
            self.handlers.clone(),
            (record.node, scope, enclosing),
        )
        .under_policy(options.output_policy)
        .with_resolver(self.resolver.clone());
        let mut failed = before.failed;
        // The restored record's assets resolve in its document's source,
        // as a replay of the whole document would resolve them.
        let place = std::fs::canonicalize(document)
            .ok()
            .map(|canonical| sources.entry_place(&canonical, &shown, true));
        loop {
            match session.next_item() {
                Ok(Some(item)) => {
                    let mut item = Item::from(item);
                    let failures = crate::assets::resolve_item(
                        def,
                        &sources,
                        place.as_ref(),
                        &shown,
                        &mut item,
                    );
                    failed |= matches!(item, Item::Diagnostic(_)) || !failures.is_empty();
                    sink(item);
                    for failure in failures {
                        sink(Item::Diagnostic(Box::new(failure)));
                    }
                }
                Ok(None) => break,
                Err(error) => return Err(ReadError::Session(error)),
            }
        }
        Ok(Restored::Bound(failed || session.failed()))
    }

    /// An entry document's text, its snapshot key, and its profile
    /// selection, as a scan of the same invocation spelling computes
    /// them; `None` when the text cannot be read.
    fn scanned(
        &self,
        entry: &Path,
        shown: &str,
        options: &ReadOptions,
    ) -> Option<(String, String, ProfileSelection)> {
        let def = self.def;
        let text = read_text(entry, shown, "document size", ORIGIN).ok()?;
        let selection = options.selection(def, shown);
        let profiles = selection_fingerprint(def, &selection);
        let key = snapshot_key(shown, stable_hash(text.as_bytes()), profiles);
        Some((text, key, selection))
    }

    /// Read the named entry documents only, giving each import request
    /// as an item without following it: local checking reads named
    /// documents only.
    pub fn read_local(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<bool, ReadError> {
        self.read_scoped(entries, options, sink, Mode::plain(false))
            .map(|outcome| outcome.failed)
    }

    fn read_scoped(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        mode: Mode,
    ) -> Result<Outcome, ReadError> {
        let entries: Vec<EntrySource> = entries
            .iter()
            .map(|entry| EntrySource::Path(entry))
            .collect();
        self.read_inner(&entries, options, sink, Sides::none(), mode)
    }

    /// One reading under `mode`: what it follows, resolves, and keeps
    /// beyond its item stream.
    fn read_inner(
        &self,
        entries: &[EntrySource<'_>],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        sides: Sides<'_, '_, '_>,
        mode: Mode,
    ) -> Result<Outcome, ReadError> {
        let threads = options.threads.unwrap_or(1);
        // A handler or a resolver is the consuming program's code: it
        // runs in document order, on the reading's own thread.
        let alone = threads < 2
            || !mode.follow
            || !self.handlers.is_empty()
            || self.resolver.is_some()
            || self.def.kind == DefinitionKind::Dialect;
        if alone {
            return self.read_with(entries, options, sink, sides, mode, None);
        }
        let (jobs, queue) = mpsc::channel::<Job<'d>>();
        let queue = Mutex::new(queue);
        std::thread::scope(|scope| {
            for _ in 1..threads {
                scope.spawn(|| {
                    // The lock is held only to take the next job.
                    loop {
                        let job = match queue.lock() {
                            Ok(queue) => queue.recv(),
                            Err(_) => break,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                });
            }
            let ahead = Ahead {
                jobs,
                width: (threads - 1) * LOOKAHEAD,
            };
            // Dropping the sender with the reading ends the workers.
            self.read_with(entries, options, sink, sides, mode, Some(ahead))
        })
    }

    /// [`DataReader::read_inner`], binding the documents an import fans
    /// out to ahead of their turn when `ahead` gives it workers.
    fn read_with(
        &self,
        entries: &[EntrySource<'_>],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        sides: Sides<'_, '_, '_>,
        mode: Mode,
        ahead: Option<Ahead<'d>>,
    ) -> Result<Outcome, ReadError> {
        let def = self.def;
        let resolving = mode.resolving;
        // Definition documents declare their own locations rule, so every
        // entry is scanned before any import runs. A schema definition's
        // entries are confined as data documents are; a dialect
        // definition's are not.
        let mut confined = vec![true; entries.len()];
        let declared = (def.kind == DefinitionKind::Dialect).then(|| {
            let mut declared = DeclaredLocations::default();
            for (entry, confined) in entries.iter().zip(confined.iter_mut()) {
                let text = match entry {
                    EntrySource::Path(path) => std::fs::read_to_string(path).ok(),
                    EntrySource::Memory { text, .. } => Some((*text).to_owned()),
                };
                *confined = text.is_some_and(|text| declared.scan(&text));
            }
            declared
        });
        // The project root and the sources are checked before anything is
        // read, every failure reported (`KDLITH-LOC005`); each override
        // is reported before the first document.
        let sources = match Sources::admit(&options.source_inputs(), ORIGIN) {
            Ok((sources, warnings)) => {
                for warning in warnings {
                    sink(Item::Diagnostic(Box::new(warning)));
                }
                sources
            }
            Err(records) => {
                for record in records {
                    sink(Item::Diagnostic(Box::new(record)));
                }
                return Ok(Outcome::failed_early());
            }
        };
        // A demand is checked before anything is read.
        let skips = match &options.demand {
            Some(demand) => Some(Arc::<[bool]>::from(
                crate::demand::skips(def, demand).map_err(ReadError::UnknownType)?,
            )),
            None => None,
        };
        // A local reading leaves imports unfollowed, and a demand leaves
        // contributions unbound, so their records are never the whole
        // picture a resolver must see.
        let resolver = self
            .contributions
            .as_ref()
            .filter(|_| resolving && skips.is_none() && def.resolution.is_some());
        let mut records: Vec<ContributionRecord> = Vec::new();
        let mut tap = |item: Item| {
            if resolver.is_some()
                && let Item::Contribution(contribution) = &item
            {
                records.push(ContributionRecord::from_bound(contribution));
            }
            sink(item);
        };
        let Sides {
            mut texts,
            mut entering,
            mut profiling,
        } = sides;
        let armed = entering.is_some();
        let mut enter = |values: &[Option<SlotValue>]| {
            if let Some(entering) = &mut entering {
                entering(values);
            }
        };
        let mut give = |text: &str| {
            if let Some(texts) = &mut texts {
                texts(text);
            }
        };
        let mut declare = |selection: &ProfileSelection| {
            if let Some(profiling) = &mut profiling {
                profiling(selection);
            }
        };
        let mut reading = Reading {
            reader: self,
            selection: ProfileSelection::Unprofiled,
            declared,
            sources,
            local: BTreeMap::new(),
            places: HashMap::new(),
            texts: &mut give,
            entering: if armed { Some(&mut enter) } else { None },
            profiling: &mut declare as Profiling<'_>,
            invocations: Invocations::new()
                .with_limit(options.max_invocations.unwrap_or(MAX_INVOCATIONS))
                .with_schema_fingerprint(self.schema_fingerprint()),
            slots: def.plan.context.layout.slots().collect(),
            mode,
            skips,
            ahead,
            held: HashMap::new(),
            seen: HashSet::new(),
            bound: HashMap::new(),
            results: Vec::new(),
            result_of: HashMap::new(),
            left: HashMap::new(),
            records: Vec::new(),
            consulted: false,
            chain: Vec::new(),
            contexts: Vec::new(),
            handlers: self.handlers.clone(),
            output_policy: options.output_policy,
            sink: &mut tap,
            failed: false,
            broken: None,
        };
        let layout = self.def.plan.context.layout.clone();
        for (entry, confined) in entries.iter().zip(confined) {
            // Selected once per entry document, before any of its nodes
            // binds, from the explicit name or set and the entry's
            // root-relative source path — never from its content. The
            // documents its imports reach bind under this selection.
            let shown = match entry {
                EntrySource::Path(path) => path.display().to_string(),
                EntrySource::Memory { shown, .. } => (*shown).to_owned(),
            };
            reading.chain.push(shown.clone());
            reading.selection = options.selection(def, &shown);
            reading
                .invocations
                .select_profiles(selection_fingerprint(def, &reading.selection));
            let entered = match entry {
                EntrySource::Path(path) => reading
                    .invocations
                    .enter_root(path, Context::new(layout.clone()))
                    .inspect(|id| reading.place_entry(*id, confined)),
                EntrySource::Memory { text, .. } => reading
                    .invocations
                    .enter_root_in_memory(&shown, Context::new(layout.clone()))
                    .inspect(|id| {
                        reading.held.insert(*id, Rc::from(*text));
                    }),
            };
            match entered {
                Ok(id) => reading.visit(id, None),
                Err(failure) => reading.diagnose(entry_failure(failure)),
            }
            if let Some(error) = reading.broken {
                return Err(error);
            }
        }
        // Each unpinned source the reading used is noted once, with its
        // tree's digest; a note never fails the reading.
        for note in reading.sources.unpinned_notes(ORIGIN) {
            (reading.sink)(Item::Diagnostic(Box::new(note)));
        }
        let entered = reading.invocations.len();
        let local = (!reading.local.is_empty())
            .then(|| lines_digest(std::mem::take(&mut reading.local).into_iter().collect()));
        let set = reading.sources.source_set(local);
        let (mut failed, mut results, contexts) =
            (reading.failed, reading.results, reading.contexts);
        for result in &mut results {
            result.sources.clone_from(&set.fingerprint);
        }
        // A failed reading's records are incomplete; resolving them
        // would report on a partial picture.
        if let Some(resolver) = resolver.filter(|_| !failed && !records.is_empty()) {
            match resolve(def, resolver.as_ref(), &records) {
                Ok(resolved) => resolved
                    .into_iter()
                    .for_each(|resolved| sink(Item::Resolution(resolved))),
                Err(error) => {
                    failed = true;
                    sink(Item::Diagnostic(Box::new(failure_record(&error, ORIGIN))));
                }
            }
        }
        sink(Item::Sources(set));
        Ok(Outcome {
            failed,
            results,
            contexts,
            entered,
        })
    }

    /// The fingerprint of the definition every invocation binds under.
    fn schema_fingerprint(&self) -> u64 {
        stable_hash(self.def.digest().as_bytes())
    }
}

/// Every slot's value as a document's first node binds under it: the
/// projected context with source-context values applied, as a binding
/// session applies them, so replay and bind enter alike.
fn entry_values(
    def: &Definition,
    context: Option<&Context>,
    shown: &str,
) -> Vec<Option<SlotValue>> {
    let mut context = context
        .cloned()
        .unwrap_or_else(|| Context::new(def.plan.context.layout.clone()));
    // A derived value that fails its type is the session's diagnostic.
    let _ = crate::source_context::apply(def, &mut context, shown);
    context.snapshot_values()
}

/// Where a reading gives what a consumer pairs with each document item,
/// just before it: the document's text, its entry context, and the
/// profile selection of the entry it belongs to.
pub(crate) struct Sides<'t, 'e, 'p> {
    pub(crate) texts: Option<&'t mut dyn FnMut(&str)>,
    pub(crate) entering: Option<Entering<'e>>,
    pub(crate) profiling: Option<Profiling<'p>>,
}

impl Sides<'_, '_, '_> {
    fn none() -> Self {
        Self {
            texts: None,
            entering: None,
            profiling: None,
        }
    }
}

/// What one reading does beyond streaming its entries' items. Everything
/// a reading keeps costs memory in proportion to the reading, so a plain
/// reading keeps nothing: each document's text and items are dropped as
/// soon as they have streamed.
#[derive(Clone, Copy, Debug)]
struct Mode {
    /// Whether import requests are followed; a local reading gives each
    /// request as an item and stops there.
    follow: bool,
    /// Whether contribution records are routed to the registered
    /// resolver: a reading of only some of an invocation's entries
    /// would resolve a partial picture.
    resolving: bool,
    /// Whether every bound result is returned, each with its items.
    results: bool,
    /// Whether the context each entry's root invocation left is
    /// returned.
    contexts: bool,
    /// Whether each entry's context carries every record its reading
    /// bound, with the context the record bound under.
    records: bool,
}

impl Mode {
    /// A reading that keeps nothing beyond its stream.
    fn plain(follow: bool) -> Self {
        Self {
            follow,
            resolving: follow,
            results: false,
            contexts: false,
            records: false,
        }
    }
}

/// A piece of work for a reading's workers.
type Job<'a> = Box<dyn FnOnce() + Send + 'a>;

/// How many documents each worker may have bound ahead of the reading:
/// enough that no worker waits on the reading, few enough that the
/// items waiting for their turn stay a small part of it.
const LOOKAHEAD: usize = 4;

/// The source identity a document is bound under ahead of its turn;
/// its items move to its invocation's identity when their turn comes.
const AHEAD: SourceId = SourceId(u32::MAX);

/// A reading's workers: where it sends the documents to bind ahead of
/// their turn, and how many it lets wait.
struct Ahead<'a> {
    jobs: mpsc::Sender<Job<'a>>,
    width: usize,
}

impl<'a> Ahead<'a> {
    /// Have a worker read and bind one document; `None` when no worker
    /// is left to take it.
    fn start(&self, prebinding: Prebinding<'a>) -> Option<mpsc::Receiver<Prebound>> {
        let (done, waiting) = mpsc::sync_channel(1);
        let job: Job<'a> = Box::new(move || {
            // A reading that stopped waiting dropped its end.
            let _ = done.send(prebinding.run());
        });
        self.jobs.send(job).ok().map(|()| waiting)
    }
}

/// One document to read and bind ahead of its turn, with everything its
/// binding reads of the reading.
struct Prebinding<'a> {
    def: &'a Definition,
    path: PathBuf,
    shown: String,
    context: Option<Context>,
    selection: ProfileSelection,
    output_policy: Option<OutputPolicy>,
    skips: Option<Arc<[bool]>>,
    /// Whether each record carries the context it bound under.
    records: bool,
    /// Whether each record and declaration carries it.
    contexts: bool,
}

impl Prebinding<'_> {
    fn run(self) -> Prebound {
        let def = self.def;
        let text = read_text(&self.path, &self.shown, "document size", ORIGIN)?;
        let mut read = Read::of(text);
        let context = self
            .context
            .unwrap_or_else(|| Context::new(def.plan.context.layout.clone()));
        let mut session = BindingSession::with_handlers(
            def,
            AHEAD,
            &self.shown,
            &read.text,
            context,
            self.selection,
            Handlers::new(),
        )
        .under_policy(self.output_policy)
        .skipping(self.skips);
        let mut items = Vec::new();
        let mut broken = None;
        loop {
            match session.next_item() {
                // The reading follows an import in the document's turn.
                Ok(Some(kdlith_model::BoundItem::Import(_))) => return Ok(read),
                Ok(Some(item)) => {
                    let carries = match &item {
                        kdlith_model::BoundItem::Record(_) => self.records || self.contexts,
                        kdlith_model::BoundItem::Declaration(_) => self.contexts,
                        _ => false,
                    };
                    let values = carries.then(|| session.context().snapshot_values());
                    items.push((item, values));
                }
                Ok(None) => break,
                Err(error) => {
                    broken = Some(error);
                    break;
                }
            }
        }
        let bound = Bound {
            items,
            failed: session.failed(),
            left: session.left_context(),
            broken,
        };
        drop(session);
        read.bound = Some(bound);
        Ok(read)
    }
}

/// A document as a worker left it, or why it could not be read.
type Prebound = Result<Read, Box<DiagnosticRecord>>;

/// One document's text, with what its reading derives from the text
/// alone and, when a worker bound it ahead of its turn, its items.
struct Read {
    text: String,
    /// The content hash the invocation identity is keyed on.
    revision: u64,
    /// The hex digest the local source's identity is computed from.
    sha: String,
    /// The document's items, when it holds no import: a document that
    /// imports binds in its turn, where its imports are followed.
    bound: Option<Bound>,
}

impl Read {
    fn of(text: String) -> Self {
        Self {
            revision: stable_hash(text.as_bytes()),
            sha: bytes_hex(text.as_bytes()),
            text,
            bound: None,
        }
    }
}

/// The items of one document bound ahead of its turn, each record and
/// declaration with the context it bound under where the reading keeps
/// it, and what its binding left.
struct Bound {
    items: Vec<(kdlith_model::BoundItem, Option<Vec<Option<SlotValue>>>)>,
    failed: bool,
    left: Vec<Option<SlotValue>>,
    broken: Option<kdlith_runtime::SessionError>,
}

/// One document's binding in progress.
struct Binding {
    id: InvocationId,
    source: SourceId,
    revision: u64,
    /// Whether the document's items are kept for replay.
    keeping: bool,
    /// Whether the document's result is its own: no import was followed.
    cacheable: bool,
    own: Vec<Item>,
    records: Vec<RecordContext>,
    bound: Vec<Vec<Option<SlotValue>>>,
}

impl Binding {
    /// Give up the result: the document followed an import.
    fn release(&mut self) {
        self.cacheable = false;
        self.keeping = false;
        self.own = Vec::new();
        self.records = Vec::new();
        self.bound = Vec::new();
    }
}

/// What a document's binding left once its stream ended.
struct Leaving {
    failed: bool,
    values: Vec<Option<SlotValue>>,
    consulted: bool,
    /// The fingerprint of the values the resolver supplied; `None` when
    /// one of them is a secret.
    resolved: Option<u64>,
}

/// What a finished reading returns beyond its item stream.
struct Outcome {
    failed: bool,
    results: Vec<BoundResult>,
    contexts: Vec<EntryContext>,
    /// How many invocations the reading entered, imports included.
    entered: usize,
}

impl Outcome {
    fn failed_early() -> Self {
        Self {
            failed: true,
            results: Vec::new(),
            contexts: Vec::new(),
            entered: 0,
        }
    }
}

/// A cached result's key: its invocation identity and the active
/// profiles it bound under.
type ResultKey = (InvocationIdentity, Vec<Sym>);

/// What a cached result's binding left beyond its items.
#[derive(Clone)]
struct Left {
    values: Vec<Option<SlotValue>>,
    consulted: bool,
    records: Vec<RecordContext>,
    /// The context each record and declaration bound under, in stream
    /// order; kept only when the reading gives them to a consumer.
    bound: Vec<Vec<Option<SlotValue>>>,
}

/// One reading in progress: the invocation stack, the caches, and the
/// sink.
struct Reading<'r, 'd> {
    reader: &'r DataReader<'d>,
    /// The profile selection of the entry document being read; every
    /// document its imports reach binds under it, and its selected
    /// profile's locations rule governs their imports.
    selection: ProfileSelection,
    /// The locations rule definition documents declare, read as they
    /// are; `None` when data documents of a compiled schema are read,
    /// whose rule the profile selection decides.
    declared: Option<DeclaredLocations>,
    /// The project root and the named sources the host registered.
    sources: Sources,
    /// Each document the reading read from no named source, by its
    /// shown path, with the hex digest of its text: what the local
    /// source's identity is computed from.
    local: BTreeMap<String, String>,
    /// Where each document read from a source sits: its imports resolve
    /// and are confined there. A document held in memory has none.
    places: HashMap<InvocationId, Place>,
    /// Where each document's text goes just before its document item.
    texts: &'r mut dyn FnMut(&str),
    /// Where the entry selection a document binds under goes, at the
    /// same point; a no-op when no consumer asked for it.
    profiling: Profiling<'r>,
    /// Where each document's entry context goes just before its text,
    /// when a consumer asked for it.
    entering: Option<Entering<'r>>,
    invocations: Invocations,
    /// Every slot of the definition, in slot order.
    slots: Vec<SlotId>,
    mode: Mode,
    /// Under a demand, the node declarations whose nodes stay unbound.
    skips: Option<Arc<[bool]>>,
    /// The workers that bind documents ahead of their turn, when the
    /// reading has any.
    ahead: Option<Ahead<'d>>,
    /// The text of each document held in memory, by its invocation, until
    /// the invocation binds it: an entry given in memory, or a file of
    /// an embedded source. It is never read from disk.
    held: HashMap<InvocationId, Rc<str>>,
    /// Every invocation semantics the reading has bound without keeping
    /// its result. A second invocation of the same semantics is kept,
    /// so a third replays: only a document invoked repeatedly ever costs
    /// the memory of its items.
    seen: HashSet<ResultKey>,
    /// The bound-result cache: one invocation's items by semantics and
    /// active profiles, so an equal repeat replays. A returning reading
    /// keeps every result; others keep one from its second invocation.
    bound: HashMap<ResultKey, Rc<Vec<Item>>>,
    /// Every result the bound-result cache took, in insertion order.
    results: Vec<BoundResult>,
    /// The position of each cached identity's result in `results`.
    result_of: HashMap<ResultKey, usize>,
    /// What each cached result's binding left beyond its items — its
    /// context, whether it consulted the value resolver, and its records'
    /// contexts — so a document replayed from the cache reports the same.
    left: HashMap<ResultKey, Left>,
    /// The records the entry document being read and its imports bound
    /// or replayed so far, and whether any of their bindings consulted
    /// the value resolver; taken when the entry's root completes.
    records: Vec<RecordContext>,
    consulted: bool,
    /// The entry documents entered so far, as the invocation spelled them.
    chain: Vec<String>,
    /// The context each entry document's root invocation left, in
    /// entry order.
    contexts: Vec<EntryContext>,
    /// The context handlers the reading installed, passed into every
    /// invocation's binding session.
    handlers: Handlers,
    /// The explicit output policy every binding session applies.
    output_policy: Option<OutputPolicy>,
    sink: &'r mut dyn FnMut(Item),
    failed: bool,
    broken: Option<ReadError>,
}

impl Reading<'_, '_> {
    fn diagnose(&mut self, record: DiagnosticRecord) {
        self.failed = true;
        (self.sink)(Item::Diagnostic(Box::new(record)));
    }

    fn emit(&mut self, item: Item) {
        if matches!(item, Item::Diagnostic(_)) {
            self.failed = true;
        }
        (self.sink)(item);
    }

    /// Bind the innermost active invocation's document, following each
    /// import as its request is read, then leave it. Nothing of the
    /// document outlives its visit but what the reading's mode keeps.
    /// `ahead` is the document as a worker read and bound it ahead of
    /// its turn, when one did.
    fn visit(&mut self, id: InvocationId, ahead: Option<Prebound>) {
        if let Some(invocation) = self.invocations.get(id) {
            let (path, shown) = (invocation.path.clone(), invocation.shown.clone());
            let context = invocation.context.project(&self.slots).ok();
            // A document held in memory reads its own text; every other
            // invocation reads from disk.
            let read = match (ahead, self.held.remove(&id)) {
                (Some(ahead), _) => ahead,
                (None, Some(held)) => Ok(Read::of(held.as_ref().to_owned())),
                (None, None) => read_text(&path, &shown, "document size", ORIGIN).map(Read::of),
            };
            match read {
                Ok(read) => self.read(id, &shown, read, context),
                Err(record) => self.diagnose(*record),
            }
        }
        self.places.remove(&id);
        self.invocations.leave();
    }

    /// Bind one invocation's text, or replay it from the bound-result
    /// cache when an invocation of equal semantics is held there.
    fn read(&mut self, id: InvocationId, shown: &str, read: Read, context: Option<Context>) {
        let Read {
            text,
            revision,
            sha,
            bound,
        } = read;
        let text: &str = &text;
        // A rule written after the import it governs still governs it,
        // so a definition document is scanned before it binds.
        if let Some(declared) = &mut self.declared {
            declared.scan(text);
        }
        if !matches!(self.places.get(&id).map(|p| &p.home), Some(Home::Named(_))) {
            self.local.insert(shown.to_owned(), sha);
        }
        self.invocations.supply_revision(id, revision);
        let identity = self.invocations.get(id).and_then(|i| i.identity());
        let digest = revision_digest(revision);
        if let Some(entering) = &mut self.entering {
            entering(&entry_values(self.reader.def, context.as_ref(), shown));
        }
        let key = identity.map(|identity| (identity, self.selection.members().to_vec()));
        let cached = key.as_ref().and_then(|key| self.bound.get(key)).cloned();
        let Some(cached) = cached else {
            let document = (shown, digest.as_str(), text);
            match bound {
                Some(bound) => self.bind_ahead(id, document, bound, key),
                None => self.bind(id, document, context, key),
            }
            return;
        };
        let mut bound = Vec::new();
        if let Some(left) = key.as_ref().and_then(|key| self.left.get(key)) {
            let Left {
                values,
                consulted,
                records,
                bound: contexts,
            } = left.clone();
            bound = contexts;
            let source = u32::try_from(id.index()).unwrap_or(u32::MAX);
            // A replay re-attributes its records to the invocation
            // replaying them, as it does its items.
            self.records.extend(records.into_iter().map(|mut record| {
                record.source = source;
                record
            }));
            self.consulted |= consulted;
            self.capture(id, shown, values);
        }
        if let Some(at) = key.and_then(|key| self.result_of.get(&key).copied()) {
            let derivation = self.derivation(id);
            self.results[at].derivations.push(derivation);
        }
        self.replay(id, (shown, &digest), text, &cached, &bound);
    }

    /// Yield a cached invocation's items under its new source identity,
    /// each record and declaration after the context it bound under.
    fn replay(
        &mut self,
        id: InvocationId,
        (shown, digest): (&str, &str),
        text: &str,
        cached: &[Item],
        bound: &[Vec<Option<SlotValue>>],
    ) {
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        self.document(source, shown, digest, text);
        let mut bound = bound.iter();
        for item in cached {
            if let Some(entering) = &mut self.entering
                && matches!(item, Item::Record(_) | Item::Declaration(_))
                && let Some(values) = bound.next()
            {
                entering(values);
            }
            let mut item = item.clone();
            remap(&mut item, source);
            let failures = self.assets(id, shown, &mut item);
            (self.sink)(item);
            for failure in failures {
                self.diagnose(failure);
            }
        }
    }

    /// Resolve the assets `item` holds in the source of the document
    /// `id` invoked, reporting each failure after the item.
    fn assets(&mut self, id: InvocationId, shown: &str, item: &mut Item) -> Vec<DiagnosticRecord> {
        if !matches!(
            item,
            Item::Record(_) | Item::Declaration(_) | Item::Contribution(_)
        ) {
            return Vec::new();
        }
        let place = self.places.get(&id);
        crate::assets::resolve_item(self.reader.def, &self.sources, place, shown, item)
    }

    /// Give a document's item, its text first, and the entry selection
    /// it binds under, to whoever asked for them.
    fn document(&mut self, source: SourceId, shown: &str, digest: &str, text: &str) {
        (self.profiling)(&self.selection);
        (self.texts)(text);
        (self.sink)(Item::Document(
            SourceDocument::new(source, shown.to_owned()).with_digest(digest),
        ));
    }

    /// Bind one document in its turn, following each import as its
    /// request is read.
    fn bind(
        &mut self,
        id: InvocationId,
        (shown, digest, text): (&str, &str, &str),
        context: Option<Context>,
        key: Option<ResultKey>,
    ) {
        let def = self.reader.def;
        let context = context.unwrap_or_else(|| Context::new(def.plan.context.layout.clone()));
        let mut binding = self.begin(id, (shown, digest, text), key.as_ref());
        let mut session = BindingSession::with_handlers(
            def,
            binding.source,
            shown,
            text,
            context,
            self.selection.clone(),
            self.handlers.clone(),
        )
        .under_policy(self.output_policy)
        .with_resolver(self.reader.resolver.clone())
        .skipping(self.skips.clone());
        while self.broken.is_none() {
            match session.next_item() {
                Ok(Some(kdlith_model::BoundItem::Import(request))) if self.mode.follow => {
                    // A followed import's items are part of this
                    // document's stream but not of its own bound result:
                    // the imports' revisions are outside this
                    // invocation's identity.
                    binding.release();
                    let mut request = request;
                    let projected = self.projection(&session, &request, shown);
                    let resolved = projected.map(|context| (context, self.targets(&request)));
                    if let Some((_, Ok(targets))) = &resolved {
                        request.targets =
                            targets.iter().map(|target| target.shown.clone()).collect();
                    }
                    self.emit(Item::Import(request.clone()));
                    if let Some((context, targets)) = resolved {
                        self.import(&request, shown, context, targets);
                    }
                }
                Ok(Some(item)) => {
                    let mut values = || session.context().snapshot_values();
                    self.take(&mut binding, shown, Item::from(item), &mut values);
                }
                Ok(None) => break,
                Err(error) => self.broken = Some(ReadError::Session(error)),
            }
        }
        let left = Leaving {
            failed: session.failed(),
            values: session.left_context(),
            consulted: session.consulted_resolver(),
            resolved: session.resolved_fingerprint(),
        };
        self.finish(binding, shown, key, left);
    }

    /// Give the items of a document a worker bound ahead of its turn,
    /// exactly as binding it in its turn gives them.
    fn bind_ahead(
        &mut self,
        id: InvocationId,
        (shown, digest, text): (&str, &str, &str),
        bound: Bound,
        key: Option<ResultKey>,
    ) {
        let mut binding = self.begin(id, (shown, digest, text), key.as_ref());
        for (item, values) in bound.items {
            let mut item = Item::from(item);
            remap(&mut item, binding.source);
            let mut values = || values.clone().unwrap_or_default();
            self.take(&mut binding, shown, item, &mut values);
        }
        if let Some(error) = bound.broken {
            self.broken = Some(ReadError::Session(error));
        }
        let left = Leaving {
            failed: bound.failed,
            values: bound.left,
            consulted: false,
            resolved: None,
        };
        self.finish(binding, shown, key, left);
    }

    /// Open one document's binding: its document item, and whether its
    /// result is kept.
    fn begin(
        &mut self,
        id: InvocationId,
        (shown, digest, text): (&str, &str, &str),
        key: Option<&ResultKey>,
    ) -> Binding {
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        let revision = self
            .invocations
            .get(id)
            .and_then(|i| i.revision)
            .unwrap_or_default();
        self.document(source, shown, digest, text);
        Binding {
            id,
            source,
            revision,
            // The result is kept for replay when the reading returns its
            // results, or when this is the second invocation of its
            // semantics; a first invocation only streams.
            keeping: key.is_some_and(|key| self.mode.results || self.seen.contains(key)),
            cacheable: true,
            own: Vec::new(),
            records: Vec::new(),
            bound: Vec::new(),
        }
    }

    /// Give one bound item of a document: resolve its assets, index it
    /// with the context it bound under where the reading keeps that, and
    /// emit it. `values` is every slot's value as the item bound.
    fn take(
        &mut self,
        binding: &mut Binding,
        shown: &str,
        mut item: Item,
        values: &mut dyn FnMut() -> Vec<Option<SlotValue>>,
    ) {
        // The bound result keeps assets as written: a replay resolves
        // them in its own document's source.
        if binding.keeping {
            binding.own.push(item.clone());
        }
        let failures = self.assets(binding.id, shown, &mut item);
        if self.mode.records
            && let Item::Record(record) = &item
        {
            // Records carry no context effects and their children scope
            // has closed, so the live context is the one the record
            // bound under.
            let indexed = RecordContext {
                identity: record_identity(&item).map(str::to_owned),
                ty: record.type_name.clone(),
                revision: binding.revision,
                source: binding.source.0,
                node: record.node.index,
                span: (record.span.start, record.span.end),
                validation: validation_status(record.validation),
                values: values(),
            };
            if binding.keeping {
                binding.records.push(indexed.clone());
            }
            self.records.push(indexed);
        }
        if let Some(entering) = &mut self.entering
            && matches!(item, Item::Record(_) | Item::Declaration(_))
        {
            // As for the record index above: no record or declaration
            // carries effects, so the live context is the one the item
            // bound under.
            let values = values();
            entering(&values);
            if binding.keeping {
                binding.bound.push(values);
            }
        }
        self.emit(item);
        for failure in failures {
            self.diagnose(failure);
        }
    }

    /// Close one document's binding: capture what its root left and,
    /// when its result is kept, cache it for replay.
    fn finish(&mut self, binding: Binding, shown: &str, key: Option<ResultKey>, left: Leaving) {
        let failed = self.failed || left.failed;
        self.failed = failed;
        let consulted = left.consulted;
        self.consulted |= consulted;
        self.capture(binding.id, shown, left.values.clone());
        if !binding.cacheable || failed || self.broken.is_some() {
            return;
        }
        let Some((identity, active)) = key else {
            return;
        };
        if !binding.keeping {
            // The next invocation of these semantics keeps its result.
            self.seen.insert((identity, active));
            return;
        }
        // Resolved values join the identity, so a changed one misses. A
        // result holding a resolved secret has no cacheable identity:
        // even a fingerprint of a secret on disk is a guessing oracle.
        let identity = if consulted {
            match left.resolved {
                Some(resolved) => identity.with_resolved(resolved),
                None => return,
            }
        } else {
            identity
        };
        let derivation = self.derivation(binding.id);
        let key = (identity, active);
        // A result the session lookup could not find before binding (its
        // resolved values were not yet known) may equal one already
        // cached: it is one result, derived twice.
        if let Some(&at) = self.result_of.get(&key) {
            self.results[at].derivations.push(derivation);
            return;
        }
        // One copy of the items, under the canonical source identity,
        // serves both the replay cache and the returned result.
        let mut own = binding.own;
        for item in &mut own {
            remap(item, SourceId(0));
        }
        let own = Rc::new(own);
        let left = Left {
            values: left.values,
            consulted,
            records: binding.records,
            bound: binding.bound,
        };
        self.left.insert(key.clone(), left);
        if self.mode.results {
            self.result_of.insert(key.clone(), self.results.len());
            self.results.push(BoundResult {
                identity,
                derivations: vec![derivation],
                items: own.clone(),
                sources: None,
            });
        }
        self.bound.insert(key, own);
    }

    /// Keep the context an entry document's root invocation left, with
    /// the records its reading bound; an imported invocation's context
    /// stays its own, and its records wait for its entry's root.
    fn capture(&mut self, id: InvocationId, shown: &str, values: Vec<Option<SlotValue>>) {
        if !self.mode.contexts {
            return;
        }
        if let Some(invocation) = self.invocations.get(id).filter(|i| i.caller.is_none()) {
            let (revision, profiles) =
                (invocation.revision.unwrap_or_default(), invocation.profiles);
            self.contexts.push(EntryContext {
                entry: shown.to_owned(),
                lineage: self.chain.clone(),
                values,
                revision,
                profiles,
                consulted_resolver: std::mem::take(&mut self.consulted),
                records: std::mem::take(&mut self.records),
                sources: self.sources.used_identities(),
            });
        }
    }

    /// Where an invocation sits: its entrypoint, itself, and its
    /// importer, each as the reading showed it.
    fn derivation(&self, id: InvocationId) -> Derivation {
        let shown = |id: InvocationId| {
            self.invocations
                .get(id)
                .map(|i| i.shown.clone())
                .unwrap_or_default()
        };
        let caller = self.invocations.get(id).and_then(|i| i.caller);
        let mut root = id;
        while let Some(up) = self.invocations.get(root).and_then(|i| i.caller) {
            root = up;
        }
        Derivation::new(shown(root), shown(id), caller.map(shown))
    }

    /// The import site's context, holding only the slots it projects.
    /// A project token names a declared slot, or a declared namespace,
    /// whose every slot is projected; a token naming neither fails
    /// closed. The resolved slot set is deduplicated.
    fn projection(
        &mut self,
        session: &BindingSession<'_, '_>,
        request: &ImportRequest,
        shown: &str,
    ) -> Option<Context> {
        let def = self.reader.def;
        let mut slots = Vec::with_capacity(request.project.len());
        for name in &request.project {
            // A name used by neither a slot nor a namespace is not in
            // the symbol table, so one lookup resolves both forms: the
            // slot, else the namespace's members.
            let found = def.names.get(name).and_then(|sym| {
                def.plan
                    .context
                    .slot(sym)
                    .map(|slot| vec![slot])
                    .or_else(|| {
                        def.plan
                            .context
                            .namespace(sym)
                            .map(|namespace| def.plan.context.layout.members(namespace).to_vec())
                    })
            });
            match found {
                Some(found) => slots.extend(found),
                None => {
                    // The code the definition loader reports for the same
                    // projection: by the kind of the importing document,
                    // which under a dialect may itself be a schema.
                    let code = match (def.kind, session.root_name()) {
                        (DefinitionKind::Dialect, Some(root))
                            if root == DefinitionKind::Dialect.root_name() =>
                        {
                            UNRESOLVED_DIALECT
                        }
                        _ => UNRESOLVED_SCHEMA,
                    };
                    let record = record(code, &["context slot", name], ORIGIN);
                    self.diagnose(at_request(record, shown, request));
                    return None;
                }
            }
        }
        slots.sort_unstable();
        slots.dedup();
        session.context().project(&slots).ok()
    }

    /// Execute one import request over the targets it resolved to:
    /// invoke each under the request's policy, in order. With workers,
    /// the documents on disk are read and bound ahead of their turn, a
    /// bounded number at a time.
    fn import(
        &mut self,
        request: &ImportRequest,
        shown: &str,
        context: Context,
        targets: Result<Vec<Target>, Box<DiagnosticRecord>>,
    ) {
        let targets = match targets {
            Ok(targets) => targets,
            Err(record) => {
                self.diagnose(at_request(*record, shown, request));
                return;
            }
        };
        let mut started: VecDeque<Option<mpsc::Receiver<Prebound>>> = VecDeque::new();
        let mut next = 0;
        for (at, target) in targets.iter().enumerate() {
            // One document alone gains nothing from a worker.
            if let Some(ahead) = self.ahead.as_ref().filter(|_| targets.len() > 1) {
                while next < targets.len() && next <= at + ahead.width {
                    let ahead_of = &targets[next];
                    let waiting = ahead_of.text.is_none().then(|| {
                        ahead.start(Prebinding {
                            def: self.reader.def,
                            path: ahead_of.resolved.clone(),
                            shown: ahead_of.shown.clone(),
                            context: context.project(&self.slots).ok(),
                            selection: self.selection.clone(),
                            output_policy: self.output_policy,
                            skips: self.skips.clone(),
                            records: self.mode.records,
                            contexts: self.entering.is_some(),
                        })
                    });
                    started.push_back(waiting.flatten());
                    next += 1;
                }
            }
            let waiting = started.pop_front().flatten();
            let admitted = self.invocations.invoke_resolved(
                &target.resolved,
                &target.shown,
                request.repeated,
                context.clone(),
            );
            match admitted {
                Ok(Admission::Invoke(id)) => {
                    self.places.insert(id, target.place.clone());
                    if let Some(text) = &target.text {
                        self.held.insert(id, text.clone());
                    }
                    // A worker that is gone leaves the document to its
                    // turn.
                    let ahead = waiting.and_then(|waiting| waiting.recv().ok());
                    self.visit(id, ahead);
                }
                Ok(_) => {}
                Err(failure) => {
                    let record = import_failure(failure, ORIGIN);
                    self.diagnose(at_request(record, shown, request));
                }
            }
            if self.broken.is_some() {
                return;
            }
        }
    }

    /// Place an entry document read from disk in its source: the local
    /// source under the project root, else one rooted at its own
    /// directory; a dialect definition document stays unconfined.
    fn place_entry(&mut self, id: InvocationId, confined: bool) {
        let Some(invocation) = self.invocations.get(id) else {
            return;
        };
        let place = self
            .sources
            .entry_place(&invocation.path, &invocation.shown, confined);
        self.places.insert(id, place);
    }

    /// The targets a request's location resolves to, from the importing
    /// document's place, under the governing locations rule: the one
    /// definition documents declare, else the selected profile's, else
    /// the schema root's.
    fn targets(&self, request: &ImportRequest) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
        let innermost = self.invocations.innermost();
        let importer = innermost.and_then(|id| {
            let place = self.places.get(&id)?;
            let invocation = self.invocations.get(id)?;
            Some(Importer {
                place,
                file: &invocation.path,
                shown: &invocation.shown,
            })
        });
        let rule = match &self.declared {
            Some(declared) => declared.rule(),
            None => self.reader.def.locations_for(self.selection.profile()),
        };
        self.sources
            .resolve(&request.expression, importer, rule, ORIGIN)
    }
}

/// An entry document the invocation stack refused. An entry is named
/// by the invocation, not requested by an import, so one that does not
/// resolve is a document that cannot be read (`KDLITH-SRC001`).
fn entry_failure(failure: ImportFailure) -> DiagnosticRecord {
    match failure {
        ImportFailure::Unresolved { target, reason } => {
            record(crate::diag::SOURCE_UNREADABLE, &[&target, &reason], ORIGIN)
        }
        failure => import_failure(failure, ORIGIN),
    }
}

/// The identity fingerprint of a resolved profile selection: its kind,
/// then each active profile's name, length-prefixed, in selection order.
fn selection_fingerprint(def: &Definition, selection: &ProfileSelection) -> u64 {
    let kind: &[u8] = match selection {
        ProfileSelection::Unprofiled => b"unprofiled",
        ProfileSelection::Selected(_) => b"selected",
        ProfileSelection::Set(_) => b"set",
        _ => b"unselected",
    };
    let mut hash = stable_hash_step(stable_hash(b"kdlith-profiles"), kind);
    for member in selection.members() {
        let name = def.names.resolve(*member).as_bytes();
        hash = stable_hash_step(hash, &(name.len() as u64).to_le_bytes());
        hash = stable_hash_step(hash, name);
    }
    hash
}

/// The index status of a bound record's validation state.
fn validation_status(state: kdlith_model::ValidationState) -> SnapshotValidation {
    match state {
        kdlith_model::ValidationState::Valid => SnapshotValidation::Validated,
        kdlith_model::ValidationState::Provisional => SnapshotValidation::Pending,
        _ => SnapshotValidation::Failed,
    }
}

/// The revision digest a document item carries: the content hash the
/// invocation identity is keyed on, stamped with its algorithm.
fn revision_digest(revision: u64) -> String {
    format!("fnv1a64:{revision:016x}")
}

/// Every provenance location of a bound item's fields and site moved
/// from the source it was bound as to the one replaying it.
fn relocate(
    fields: &mut [kdlith_model::Field],
    site: &mut Option<kdlith_model::Location>,
    from: SourceId,
    to: SourceId,
) {
    for field in fields {
        field.trace.relocate(from, to);
        relocate_value(&mut field.value, from, to);
    }
    if let Some(site) = site {
        site.relocate(from, to);
    }
}

/// Every reference site within a bound value moved, map keys included:
/// a replayed reference is written where the replaying document wrote it.
fn relocate_value(value: &mut kdlith_model::Value, from: SourceId, to: SourceId) {
    match value {
        kdlith_model::Value::Reference(reference) => reference.site.relocate(from, to),
        kdlith_model::Value::List(values) => {
            for value in values {
                relocate_value(value, from, to);
            }
        }
        kdlith_model::Value::Map(entries) => {
            for entry in entries {
                if let Some(reference) = &mut entry.reference {
                    reference.site.relocate(from, to);
                }
                relocate_value(&mut entry.value, from, to);
            }
        }
        _ => {}
    }
}

/// Move an item to a new source identity: the one identity a replay
/// changes, so a replayed document's items — and the provenance
/// locations within that document — attribute to the invocation that
/// replays them.
fn remap(item: &mut Item, source: SourceId) {
    match item {
        Item::Document(document) => document.source = source,
        Item::Record(record) => {
            let from = record.node.source;
            record.node.source = source;
            if let Some(parent) = &mut record.parent {
                parent.source = source;
            }
            relocate(&mut record.fields, &mut record.site, from, source);
        }
        Item::Declaration(declaration) => {
            let from = declaration.node.source;
            declaration.node.source = source;
            if let Some(parent) = &mut declaration.parent {
                parent.source = source;
            }
            relocate(&mut declaration.fields, &mut declaration.site, from, source);
        }
        Item::Contribution(contribution) => {
            let from = contribution.node.source;
            contribution.node.source = source;
            contribution.target.site.relocate(from, source);
            relocate_value(&mut contribution.value, from, source);
            relocate(
                &mut contribution.members,
                &mut contribution.site,
                from,
                source,
            );
        }
        Item::Import(request) => {
            let from = request.node.source;
            request.node.source = source;
            request.site.relocate(from, source);
        }
        Item::PendingBinding(pending) => {
            let from = pending.node.source;
            pending.node.source = source;
            pending.against.site.relocate(from, source);
            relocate_value(&mut pending.raw, from, source);
        }
        Item::Expansion(expansion) => {
            let from = expansion.node.source;
            expansion.node.source = source;
            expansion.target.site.relocate(from, source);
            expansion.site.relocate(from, source);
        }
        Item::ScopeComplete(completion) => {
            *completion = kdlith_model::ScopeCompletion::new(source, completion.scope);
        }
        // A diagnostic names its document by path, and an item variant
        // this version's replay does not know keeps its original
        // identity; neither silently disappears.
        _ => {}
    }
}

/// A record located at an import request's site.
fn at_request(record: DiagnosticRecord, shown: &str, request: &ImportRequest) -> DiagnosticRecord {
    let start = request.site.span.start;
    record.with_source(shown).with_span(
        request.site.span.byte_span(),
        Some(start.line),
        Some(start.column),
    )
}
