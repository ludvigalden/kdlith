//! The data-path reader: binds data documents through a compiled
//! definition and executes their imports as invocations, under the
//! identity and cache rules the runtime records.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{ImportRequest, Item, SourceDocument, SourceId};
use kdlith_runtime::{
    Admission, Context, ContributionRecord, Derivation, Handlers, InvocationId, InvocationIdentity,
    Invocations, SlotId, SlotValue, snapshot_key, stable_hash, stable_hash_step,
};

use crate::anchors::{self, Base, DeclaredGrammars, Target};
use crate::bind::{BindingSession, dispatch_scope};
use crate::definition::{Definition, DefinitionKind, ProfileSelection};
use crate::diag::{UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA, record};
use crate::discovery;
use crate::documents::{import_failure, read_text};
use crate::list::record_identity;
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

/// What one reading supplies beyond its definition: the profile, or the
/// profile set, the invocation names, and the host-supplied values of the discovery
/// anchors the active grammar declares. A grammar-declared anchor the
/// host does not supply fails closed (`KDLITH-IMP005`), never
/// degrading to an unanchored search; so does a supplied anchor no
/// grammar of the definition declares.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ReadOptions {
    profile: Option<String>,
    profiles: Vec<String>,
    anchors: Vec<(String, PathBuf)>,
    output_policy: Option<OutputPolicy>,
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
    /// else the first listed. A member the definition does not declare
    /// fails closed (`KDLITH-SCH011`). An explicitly named profile
    /// takes precedence over the set.
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

    /// Supply one discovery anchor: the typed path a grammar-declared
    /// anchor of that name resolves to.
    #[must_use]
    pub fn with_anchor(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.anchors.push((name.into(), path.into()));
        self
    }

    /// Supply exactly `anchors`, in place of any supplied before.
    pub(crate) fn set_anchors(&mut self, anchors: &[(String, PathBuf)]) {
        self.anchors = anchors.to_vec();
    }
}

/// A data document held in memory instead of read from disk: the path
/// its provenance shows, and its text. The revision digest and
/// invocation identity derive from the text exactly as a file
/// reading's do, so a `Source` holding a file's bytes and that file
/// read as an entry produce one reading. A document held in memory has
/// no directory to resolve an import against: an import inside it
/// fails closed (`KDLITH-IMP001`).
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
    pub items: Vec<Item>,
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
    /// bound under. An entry may be a document held in memory.
    pub(crate) fn read_documents(
        &self,
        entries: &[EntrySource<'_>],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        texts: &mut dyn FnMut(&str),
        entered: Option<Entering<'_>>,
        follow: bool,
    ) -> Result<bool, ReadError> {
        let sides = Sides {
            texts: Some(texts),
            entering: entered,
        };
        self.read_inner(entries, options, sink, sides, follow, follow)
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
        self.read_scoped(entries, options, sink, true)
            .map(|outcome| outcome.failed)
    }

    /// Read as [`DataReader::read`] does, over documents held in memory
    /// instead of read from disk: each `Source`'s text binds as its
    /// shown path's document would, with the same revision digest and
    /// provenance. An import inside an in-memory document fails closed
    /// (`KDLITH-IMP001`): it has no directory to resolve against.
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
        self.read_inner(&entries, options, sink, Sides::none(), true, true)
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
        self.read_scoped(entries, options, sink, true)
            .map(|outcome| (outcome.failed, outcome.results))
    }

    /// Read as [`DataReader::read`] does, and also return the context
    /// each entry document's root invocation left, in entry order. An
    /// entry whose document could not be read has none.
    pub fn read_entry_contexts(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
    ) -> Result<(bool, Vec<EntryContext>), ReadError> {
        self.read_scoped(entries, options, sink, true)
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
        self.read_scoped(entries, options, sink, true)
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
        // A reading that rejects its anchors reports it; so does replay.
        if !anchors::undeclared(&options.anchors, &def.discoveries, &def.names, ORIGIN).is_empty() {
            return Ok(Restored::Miss);
        }
        let chain: Vec<String> = entries.iter().map(|e| e.display().to_string()).collect();
        let shown = document.display().to_string();
        let Some((text, key, selection)) = self.scanned(document, &shown, options) else {
            return Ok(Restored::Miss);
        };
        let (record, snapshot) = match load_snapshot(state, &key, identity) {
            Ok(SnapshotLoad::Hit(hit)) => {
                let (_, record, snapshot) = *hit;
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
        let before = self.read_inner(&earlier, options, &mut tap, Sides::none(), true, false)?;
        let source = SourceId(u32::try_from(before.entered).unwrap_or(u32::MAX));
        let Some(scope) = dispatch_scope(def, &text, record.node) else {
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
            (record.node, scope),
        )
        .under_policy(options.output_policy)
        .with_resolver(self.resolver.clone());
        let mut failed = before.failed;
        loop {
            match session.next_item() {
                Ok(Some(item)) => {
                    let item = Item::from(item);
                    failed |= matches!(item, Item::Diagnostic(_));
                    sink(item);
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
        let selection = def.select_profile(options.profile.as_deref(), &options.profiles, shown);
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
        self.read_scoped(entries, options, sink, false)
            .map(|outcome| outcome.failed)
    }

    fn read_scoped(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        follow: bool,
    ) -> Result<Outcome, ReadError> {
        let entries: Vec<EntrySource> = entries
            .iter()
            .map(|entry| EntrySource::Path(entry))
            .collect();
        self.read_inner(&entries, options, sink, Sides::none(), follow, follow)
    }

    /// [`DataReader::read_scoped`], with contribution resolution chosen
    /// separately: a reading of only some of an invocation's entries
    /// would resolve a partial picture.
    fn read_inner(
        &self,
        entries: &[EntrySource<'_>],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        sides: Sides<'_, '_>,
        follow: bool,
        resolving: bool,
    ) -> Result<Outcome, ReadError> {
        let def = self.def;
        // Definition documents declare their own discovery grammars, so
        // every entry is scanned before any import runs: the compiler
        // validates expressions against the full set.
        let grammars = (def.kind == DefinitionKind::Dialect).then(|| {
            let mut grammars = DeclaredGrammars::default();
            for entry in entries {
                let text = match entry {
                    EntrySource::Path(path) => std::fs::read_to_string(path).ok(),
                    EntrySource::Memory { text, .. } => Some((*text).to_owned()),
                };
                if let Some(text) = text {
                    grammars.scan(&text);
                }
            }
            grammars
        });
        // A host-supplied anchor no discovery grammar of the definition
        // declares is rejected before anything is read, never silently
        // ignored: the mirror of a declared anchor the host leaves
        // unsupplied (`KDLITH-IMP005` both ways).
        let undeclared = match &grammars {
            Some(declared) => anchors::undeclared(
                &options.anchors,
                &declared.grammars,
                &declared.names,
                ORIGIN,
            ),
            None => anchors::undeclared(&options.anchors, &def.discoveries, &def.names, ORIGIN),
        };
        if !undeclared.is_empty() {
            for record in undeclared {
                sink(Item::Diagnostic(Box::new(record)));
            }
            return Ok(Outcome {
                failed: true,
                results: Vec::new(),
                contexts: Vec::new(),
                entered: 0,
            });
        }
        let anchors = anchors::anchor_map(&options.anchors, &def.names);
        // A local reading leaves imports unfollowed, so its records are
        // never the whole picture a resolver must see.
        let resolver = self
            .contributions
            .as_ref()
            .filter(|_| resolving && def.resolution.is_some());
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
        let mut reading = Reading {
            reader: self,
            selection: ProfileSelection::Unprofiled,
            anchors,
            grammars,
            supplied: options.anchors.clone(),
            texts: &mut give,
            entering: if armed { Some(&mut enter) } else { None },
            invocations: Invocations::new().with_schema_fingerprint(self.schema_fingerprint()),
            syntax: HashMap::new(),
            held: HashMap::new(),
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
            follow,
        };
        let layout = self.def.plan.context.layout.clone();
        for entry in entries {
            // Selected once per entry document, before any of its nodes
            // binds, from the explicit name or set and the entry's
            // source path as the invocation spells it — never from its
            // content. The documents its imports reach bind under this
            // selection.
            let shown = match entry {
                EntrySource::Path(path) => path.display().to_string(),
                EntrySource::Memory { shown, .. } => (*shown).to_owned(),
            };
            reading.chain.push(shown.clone());
            reading.selection =
                def.select_profile(options.profile.as_deref(), &options.profiles, &shown);
            reading
                .invocations
                .select_profiles(selection_fingerprint(def, &reading.selection));
            let entered = match entry {
                EntrySource::Path(path) => reading
                    .invocations
                    .enter_root(path, Context::new(layout.clone())),
                EntrySource::Memory { text, .. } => reading
                    .invocations
                    .enter_root_in_memory(&shown, Context::new(layout.clone()))
                    .inspect(|id| {
                        reading.held.insert(*id, Rc::from(*text));
                    }),
            };
            match entered {
                Ok(id) => reading.visit(id),
                Err(failure) => reading.diagnose(import_failure(failure, ORIGIN)),
            }
            if let Some(error) = reading.broken {
                return Err(error);
            }
        }
        let entered = reading.invocations.len();
        let (mut failed, results, contexts) = (reading.failed, reading.results, reading.contexts);
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
/// just before it: the document's text, and its entry context.
struct Sides<'t, 'e> {
    texts: Option<&'t mut dyn FnMut(&str)>,
    entering: Option<Entering<'e>>,
}

impl Sides<'_, '_> {
    fn none() -> Self {
        Self {
            texts: None,
            entering: None,
        }
    }
}

/// What a finished reading returns beyond its item stream.
struct Outcome {
    failed: bool,
    results: Vec<BoundResult>,
    contexts: Vec<EntryContext>,
    /// How many invocations the reading entered, imports included.
    entered: usize,
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
    /// profile's grammar governs their import expressions.
    selection: ProfileSelection,
    anchors: BTreeMap<Sym, PathBuf>,
    /// The discovery grammars definition documents declare, gathered in
    /// reading order; `None` when data documents of a compiled schema
    /// are read, whose grammar the profile selection binds.
    grammars: Option<DeclaredGrammars>,
    /// The anchors the host supplied, as spelled: definition documents
    /// intern their grammars' names as they are read.
    supplied: Vec<(String, PathBuf)>,
    /// Where each document's text goes just before its document item.
    texts: &'r mut dyn FnMut(&str),
    /// Where each document's entry context goes just before its text,
    /// when a consumer asked for it.
    entering: Option<Entering<'r>>,
    invocations: Invocations,
    /// The syntax cache: document text by source revision, so a source
    /// invoked again under the same revision is read and stored once.
    syntax: HashMap<u64, Rc<str>>,
    /// The text of each entry document held in memory, by its
    /// invocation: it is never read from disk, and an import inside it
    /// has no directory to resolve against.
    held: HashMap<InvocationId, Rc<str>>,
    /// The bound-result cache: the items of one invocation by their
    /// invocation semantics and the active profiles they bound under,
    /// so a repeated invocation with equal semantics under the same
    /// selection replays instead of rebinding.
    bound: HashMap<(InvocationIdentity, Vec<Sym>), Rc<Vec<Item>>>,
    /// Every result the bound-result cache took, in insertion order.
    results: Vec<BoundResult>,
    /// The position of each cached identity's result in `results`.
    result_of: HashMap<(InvocationIdentity, Vec<Sym>), usize>,
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
    /// Whether import requests are followed; a local reading gives each
    /// request as an item and stops there.
    follow: bool,
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
    /// import as its request is read, then leave it.
    fn visit(&mut self, id: InvocationId) {
        if let Some(invocation) = self.invocations.get(id) {
            let (path, shown) = (invocation.path.clone(), invocation.shown.clone());
            let context = invocation.context.project(&self.all_slots());
            // A document held in memory reads its own text; every other
            // invocation reads from disk.
            let read = match self.held.get(&id) {
                Some(held) => Ok(held.clone()),
                None => read_text(&path, &shown, "document size", ORIGIN)
                    .map(|text| Rc::from(text.as_str())),
            };
            match read {
                Ok(text) => {
                    // A grammar written after the import it governs
                    // still governs it, so a definition document is
                    // scanned before it binds.
                    if let Some(grammars) = &mut self.grammars {
                        grammars.scan(&text);
                    }
                    let revision = stable_hash(text.as_bytes());
                    self.invocations.supply_revision(id, revision);
                    let text = self.syntax.entry(revision).or_insert_with(|| text).clone();
                    let identity = self.invocations.get(id).and_then(|i| i.identity());
                    let digest = revision_digest(revision);
                    let active = self.selection.members().to_vec();
                    if let Some(entering) = &mut self.entering {
                        entering(&entry_values(
                            self.reader.def,
                            context.as_ref().ok(),
                            &shown,
                        ));
                    }
                    if let Some(cached) =
                        identity.and_then(|k| self.bound.get(&(k, active.clone())))
                    {
                        let cached = cached.clone();
                        let key = identity.map(|k| (k, active));
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
                            // A replay re-attributes its records to the
                            // invocation replaying them, as it does its items.
                            self.records.extend(records.into_iter().map(|mut record| {
                                record.source = source;
                                record
                            }));
                            self.consulted |= consulted;
                            self.capture(id, &shown, values);
                        }
                        if let Some(at) = key.and_then(|key| self.result_of.get(&key).copied()) {
                            let derivation = self.derivation(id);
                            self.results[at].derivations.push(derivation);
                        }
                        self.replay(id, (&shown, &digest), &text, cached, &bound);
                    } else {
                        self.bind(id, (&shown, &digest), &text, context.ok(), identity);
                    }
                }
                Err(record) => self.diagnose(*record),
            }
        }
        self.invocations.leave();
    }

    /// Yield a cached invocation's items under its new source identity,
    /// each record and declaration after the context it bound under.
    fn replay(
        &mut self,
        id: InvocationId,
        (shown, digest): (&str, &str),
        text: &str,
        cached: Rc<Vec<Item>>,
        bound: &[Vec<Option<SlotValue>>],
    ) {
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        self.document(source, shown, digest, text);
        let mut bound = bound.iter();
        for item in cached.iter() {
            if let Some(entering) = &mut self.entering
                && matches!(item, Item::Record(_) | Item::Declaration(_))
                && let Some(values) = bound.next()
            {
                entering(values);
            }
            (self.sink)(remap(item, source));
        }
    }

    /// Give a document's item, its text first to whoever asked for it.
    fn document(&mut self, source: SourceId, shown: &str, digest: &str, text: &str) {
        (self.texts)(text);
        (self.sink)(Item::Document(
            SourceDocument::new(source, shown.to_owned()).with_digest(digest),
        ));
    }

    fn bind(
        &mut self,
        id: InvocationId,
        (shown, digest): (&str, &str),
        text: &str,
        context: Option<Context>,
        identity: Option<InvocationIdentity>,
    ) {
        let def = self.reader.def;
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        let context = context.unwrap_or_else(|| Context::new(def.plan.context.layout.clone()));
        let revision = self
            .invocations
            .get(id)
            .and_then(|i| i.revision)
            .unwrap_or_default();
        self.document(source, shown, digest, text);
        let mut session = BindingSession::with_handlers(
            def,
            source,
            shown,
            text,
            context,
            self.selection.clone(),
            self.handlers.clone(),
        )
        .under_policy(self.output_policy)
        .with_resolver(self.reader.resolver.clone());
        let mut own: Vec<Item> = Vec::new();
        let mut records: Vec<RecordContext> = Vec::new();
        let mut bound: Vec<Vec<Option<SlotValue>>> = Vec::new();
        let mut cacheable = true;
        while self.broken.is_none() {
            match session.next_item() {
                Ok(Some(item)) => {
                    if let kdlith_model::BoundItem::Import(request) = &item {
                        if self.follow {
                            // A followed import's items are part of this
                            // document's stream but not of its own bound
                            // result: the imports' revisions are outside
                            // this invocation's identity.
                            cacheable = false;
                            let mut request = request.clone();
                            let projected = self.projection(&session, &request, shown);
                            let resolved =
                                projected.map(|context| (context, self.targets(&request)));
                            if let Some((_, Ok(targets))) = &resolved {
                                request.targets =
                                    targets.iter().map(|target| target.shown.clone()).collect();
                            }
                            self.emit(Item::Import(request.clone()));
                            if let Some((context, targets)) = resolved {
                                self.import(&request, shown, context, targets);
                            }
                            continue;
                        }
                    }
                    let item = Item::from(item);
                    if let Item::Record(record) = &item {
                        // Records carry no context effects and their
                        // children scope has closed, so the live context is
                        // the one the record bound under.
                        let indexed = RecordContext {
                            identity: record_identity(&item).map(str::to_owned),
                            ty: record.type_name.clone(),
                            revision,
                            source: source.0,
                            node: record.node.index,
                            span: (record.span.start, record.span.end),
                            validation: validation_status(record.validation),
                            values: session.context().snapshot_values(),
                        };
                        self.records.push(indexed.clone());
                        records.push(indexed);
                    }
                    if cacheable {
                        own.push(item.clone());
                    }
                    if let Some(entering) = &mut self.entering
                        && matches!(item, Item::Record(_) | Item::Declaration(_))
                    {
                        // As for the record index above: no record or
                        // declaration carries effects, so the live
                        // context is the one the item bound under.
                        let values = session.context().snapshot_values();
                        entering(&values);
                        bound.push(values);
                    }
                    self.emit(item);
                }
                Ok(None) => break,
                Err(error) => self.broken = Some(ReadError::Session(error)),
            }
        }
        let failed = self.failed || session.failed();
        self.failed = failed;
        let left = session.left_context();
        let consulted = session.consulted_resolver();
        self.consulted |= consulted;
        self.capture(id, shown, left.clone());
        // Resolved values join the identity, so a changed one misses. A
        // result holding a resolved secret has no cacheable identity:
        // even a fingerprint of a secret on disk is a guessing oracle.
        let identity = match identity {
            Some(identity) if consulted => session
                .resolved_fingerprint()
                .map(|resolved| identity.with_resolved(resolved)),
            other => other,
        };
        if cacheable && identity.is_some() && self.broken.is_none() {
            if let Some(identity) = identity.filter(|_| !failed) {
                let derivation = self.derivation(id);
                let active = self.selection.members().to_vec();
                let key = (identity, active);
                // A result the session lookup could not find before
                // binding (its resolved values were not yet known) may
                // equal one already cached: it is one result, derived
                // twice.
                if let Some(&at) = self.result_of.get(&key) {
                    self.results[at].derivations.push(derivation);
                    return;
                }
                self.result_of.insert(key.clone(), self.results.len());
                let left = Left {
                    values: left,
                    consulted,
                    records,
                    bound,
                };
                self.left.insert(key.clone(), left);
                self.results.push(BoundResult {
                    identity,
                    derivations: vec![derivation],
                    items: own.iter().map(|item| remap(item, SourceId(0))).collect(),
                });
                self.bound.insert(key, Rc::new(own));
            }
        }
    }

    /// Keep the context an entry document's root invocation left, with
    /// the records its reading bound; an imported invocation's context
    /// stays its own, and its records wait for its entry's root.
    fn capture(&mut self, id: InvocationId, shown: &str, values: Vec<Option<SlotValue>>) {
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
    fn projection(
        &mut self,
        session: &BindingSession<'_, '_>,
        request: &ImportRequest,
        shown: &str,
    ) -> Option<Context> {
        let def = self.reader.def;
        let mut slots = Vec::with_capacity(request.project.len());
        for name in &request.project {
            match def
                .names
                .get(name)
                .and_then(|sym| def.plan.context.slot(sym))
            {
                Some(slot) => slots.push(slot),
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
        session.context().project(&slots).ok()
    }

    /// Execute one import request over the targets it resolved to:
    /// invoke each under the request's policy.
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
        for target in targets {
            let admitted = self.invocations.invoke_resolved(
                &target.resolved,
                &target.shown,
                request.repeated,
                context.clone(),
            );
            match admitted {
                Ok(Admission::Invoke(id)) => self.visit(id),
                Ok(_) => {}
                Err(failure) => {
                    let record = import_failure(failure, ORIGIN);
                    self.diagnose(at_request(record, shown, request));
                }
            }
        }
    }

    /// The targets a request resolves to: the literal path relative to
    /// its importer, or the matches of a grammar-admitted expression
    /// against the host-supplied anchors, in deterministic order.
    fn targets(&self, request: &ImportRequest) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
        let importer = self.invocations.innermost();
        // A document held in memory has no directory to resolve an
        // import against: the request fails closed rather than resolve
        // against a path it merely spells.
        if importer.is_some_and(|id| self.held.contains_key(&id)) {
            return Err(Box::new(unresolvable(&request.expression)));
        }
        if !discovery::is_expression(&request.expression) {
            let Some(importer) = importer else {
                return Err(Box::new(unresolvable(&request.expression)));
            };
            let invocation = self
                .invocations
                .get(importer)
                .expect("the innermost invocation is registered");
            let shown = Path::new(&invocation.shown)
                .parent()
                .unwrap_or(Path::new(""))
                .join(&request.expression)
                .display()
                .to_string();
            let base = invocation.path.parent().unwrap_or(Path::new(""));
            let resolved = std::fs::canonicalize(base.join(&request.expression))
                .ok()
                .filter(|resolved| resolved.is_file());
            return match resolved {
                Some(resolved) => Ok(vec![Target { resolved, shown }]),
                None => Err(Box::new(unresolvable(&shown))),
            };
        }
        let def = self.reader.def;
        let dir = importer
            .and_then(|id| self.invocations.get(id))
            .map(|invocation| invocation.path.parent().unwrap_or(Path::new("")).to_owned())
            .unwrap_or_default();
        if let Some(grammars) = &self.grammars {
            // A definition document shows a match relative to its
            // importer as the reading showed it.
            let shown = importer
                .and_then(|id| self.invocations.get(id))
                .map(|invocation| {
                    Path::new(&invocation.shown)
                        .parent()
                        .unwrap_or(Path::new(""))
                        .to_owned()
                })
                .unwrap_or_default();
            let anchors = anchors::anchor_map(&self.supplied, &grammars.names);
            let base = Base {
                dir: &dir,
                shown: &shown,
            };
            return anchors::expression_targets(
                &request.expression,
                grammars.grammar(),
                &grammars.names,
                &anchors,
                &base,
                ORIGIN,
            );
        }
        // The data path has always shown a match as it was found.
        let base = Base {
            dir: &dir,
            shown: &dir,
        };
        anchors::expression_targets(
            &request.expression,
            def.grammar_for(self.selection.profile()),
            &def.names,
            &self.anchors,
            &base,
            ORIGIN,
        )
    }

    /// Every slot of the definition: an invocation's own context is
    /// already its caller's projection.
    fn all_slots(&self) -> Vec<SlotId> {
        self.reader.def.plan.context.layout.slots().collect()
    }
}

/// An import target that does not resolve (`KDLITH-IMP001`).
fn unresolvable(target: &str) -> DiagnosticRecord {
    record(crate::diag::IMPORT_UNRESOLVED, &[target], ORIGIN)
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

/// An item under a new source identity: the one identity a replay
/// changes, so a replayed document's items — and the provenance
/// locations within that document — attribute to the invocation that
/// replayed them.
fn remap(item: &Item, source: SourceId) -> Item {
    match item {
        Item::Document(document) => {
            let mut document = document.clone();
            document.source = source;
            Item::Document(document)
        }
        Item::Record(record) => {
            let mut record = record.clone();
            let from = record.node.source;
            record.node.source = source;
            relocate(&mut record.fields, &mut record.site, from, source);
            Item::Record(record)
        }
        Item::Declaration(declaration) => {
            let mut declaration = declaration.clone();
            let from = declaration.node.source;
            declaration.node.source = source;
            relocate(&mut declaration.fields, &mut declaration.site, from, source);
            Item::Declaration(declaration)
        }
        Item::Contribution(contribution) => {
            let mut contribution = contribution.clone();
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
            Item::Contribution(contribution)
        }
        Item::Import(request) => {
            let mut request = request.clone();
            let from = request.node.source;
            request.node.source = source;
            request.site.relocate(from, source);
            Item::Import(request)
        }
        Item::PendingBinding(pending) => {
            let mut pending = pending.clone();
            let from = pending.node.source;
            pending.node.source = source;
            pending.against.site.relocate(from, source);
            relocate_value(&mut pending.raw, from, source);
            Item::PendingBinding(pending)
        }
        Item::Expansion(expansion) => {
            let mut expansion = expansion.clone();
            let from = expansion.node.source;
            expansion.node.source = source;
            expansion.target.site.relocate(from, source);
            expansion.site.relocate(from, source);
            Item::Expansion(expansion)
        }
        Item::Diagnostic(record) => Item::Diagnostic(record.clone()),
        Item::ScopeComplete(completion) => {
            Item::ScopeComplete(kdlith_model::ScopeCompletion::new(source, completion.scope))
        }
        // An item variant this version's replay does not know keeps its
        // original identity; it never silently disappears.
        other => other.clone(),
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
