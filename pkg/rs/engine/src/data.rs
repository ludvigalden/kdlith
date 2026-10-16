//! The data-path reader: binds data documents through a compiled
//! definition and executes their imports as invocations, under the
//! identity and cache rules the runtime records.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{ImportRequest, Item, SourceDocument, SourceId};
use kdlith_runtime::{
    Admission, Context, Derivation, Handlers, InvocationId, InvocationIdentity, Invocations,
    SlotId, stable_hash, stable_hash_step,
};

use crate::anchors::{self, Base, Target};
use crate::bind::BindingSession;
use crate::definition::{Definition, ProfileSelection};
use crate::diag::{UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA, record};
use crate::discovery;
use crate::documents::{import_failure, read_text};
use crate::names::Sym;
use crate::typed::ReadError;
use crate::types::OutputPolicy;
use crate::value_resolver::SharedResolver;

const ORIGIN: &str = "data-document";

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
}

impl<'d> DataReader<'d> {
    pub fn new(def: &'d Definition) -> Self {
        Self {
            def,
            handlers: Handlers::new(),
            resolver: None,
        }
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

    /// The definition the reader binds under.
    pub fn definition(&self) -> &'d Definition {
        self.def
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
            .map(|(failed, _)| failed)
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
            .map(|(failed, _)| failed)
    }

    fn read_scoped(
        &self,
        entries: &[&Path],
        options: &ReadOptions,
        sink: &mut dyn FnMut(Item),
        follow: bool,
    ) -> Result<(bool, Vec<BoundResult>), ReadError> {
        // A host-supplied anchor no discovery grammar of the definition
        // declares is rejected before anything is read, never silently
        // ignored: the mirror of a declared anchor the host leaves
        // unsupplied (`KDLITH-IMP005` both ways).
        let def = self.def;
        let undeclared =
            anchors::undeclared(&options.anchors, &def.discoveries, &def.names, ORIGIN);
        if !undeclared.is_empty() {
            for record in undeclared {
                sink(Item::Diagnostic(Box::new(record)));
            }
            return Ok((true, Vec::new()));
        }
        let anchors = anchors::anchor_map(&options.anchors, &def.names);
        let mut reading = Reading {
            reader: self,
            selection: ProfileSelection::Unprofiled,
            anchors,
            invocations: Invocations::new().with_schema_fingerprint(self.schema_fingerprint()),
            syntax: HashMap::new(),
            bound: HashMap::new(),
            results: Vec::new(),
            result_of: HashMap::new(),
            handlers: self.handlers.clone(),
            output_policy: options.output_policy,
            sink,
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
            let shown = entry.display().to_string();
            reading.selection =
                def.select_profile(options.profile.as_deref(), &options.profiles, &shown);
            reading
                .invocations
                .select_profiles(selection_fingerprint(def, &reading.selection));
            match reading
                .invocations
                .enter_root(entry, Context::new(layout.clone()))
            {
                Ok(id) => reading.visit(id),
                Err(failure) => reading.diagnose(import_failure(failure, ORIGIN)),
            }
            if let Some(error) = reading.broken {
                return Err(error);
            }
        }
        Ok((reading.failed, reading.results))
    }

    /// The fingerprint of the definition every invocation binds under.
    fn schema_fingerprint(&self) -> u64 {
        stable_hash(self.def.digest().as_bytes())
    }
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
    invocations: Invocations,
    /// The syntax cache: document text by source revision, so a source
    /// invoked again under the same revision is read and stored once.
    syntax: HashMap<u64, Rc<str>>,
    /// The bound-result cache: the items of one invocation by their
    /// invocation semantics and the active profiles they bound under,
    /// so a repeated invocation with equal semantics under the same
    /// selection replays instead of rebinding.
    bound: HashMap<(InvocationIdentity, Vec<Sym>), Rc<Vec<Item>>>,
    /// Every result the bound-result cache took, in insertion order.
    results: Vec<BoundResult>,
    /// The position of each cached identity's result in `results`.
    result_of: HashMap<(InvocationIdentity, Vec<Sym>), usize>,
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
            match read_text(&path, &shown, "document size", ORIGIN) {
                Ok(text) => {
                    let revision = stable_hash(text.as_bytes());
                    self.invocations.supply_revision(id, revision);
                    let text = self
                        .syntax
                        .entry(revision)
                        .or_insert_with(|| Rc::from(text.as_str()))
                        .clone();
                    let identity = self.invocations.get(id).and_then(|i| i.identity());
                    let digest = revision_digest(revision);
                    let active = self.selection.members().to_vec();
                    if let Some(cached) =
                        identity.and_then(|k| self.bound.get(&(k, active.clone())))
                    {
                        let cached = cached.clone();
                        if let Some(at) =
                            identity.and_then(|k| self.result_of.get(&(k, active)).copied())
                        {
                            let derivation = self.derivation(id);
                            self.results[at].derivations.push(derivation);
                        }
                        self.replay(id, &shown, &digest, cached);
                    } else {
                        self.bind(id, (&shown, &digest), &text, context.ok(), identity);
                    }
                }
                Err(record) => self.diagnose(*record),
            }
        }
        self.invocations.leave();
    }

    /// Yield a cached invocation's items under its new source identity.
    fn replay(&mut self, id: InvocationId, shown: &str, digest: &str, cached: Rc<Vec<Item>>) {
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        (self.sink)(Item::Document(
            SourceDocument::new(source, shown.to_owned()).with_digest(digest),
        ));
        for item in cached.iter() {
            (self.sink)(remap(item, source));
        }
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
        (self.sink)(Item::Document(
            SourceDocument::new(source, shown.to_owned()).with_digest(digest),
        ));
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
                            let request = request.clone();
                            let projected = self.projection(&session, &request, shown);
                            self.emit(Item::Import(request.clone()));
                            if let Some(context) = projected {
                                self.import(&request, shown, context);
                            }
                            continue;
                        }
                    }
                    let item = Item::from(item);
                    if cacheable {
                        own.push(item.clone());
                    }
                    self.emit(item);
                }
                Ok(None) => break,
                Err(error) => self.broken = Some(ReadError::Session(error)),
            }
        }
        let failed = self.failed || session.failed();
        self.failed = failed;
        // Resolved values join the identity, so a changed one misses. A
        // result holding a resolved secret has no cacheable identity:
        // even a fingerprint of a secret on disk is a guessing oracle.
        let identity = match identity {
            Some(identity) if session.consulted_resolver() => session
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
                self.results.push(BoundResult {
                    identity,
                    derivations: vec![derivation],
                    items: own.iter().map(|item| remap(item, SourceId(0))).collect(),
                });
                self.bound.insert(key, Rc::new(own));
            }
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
                    let code = match def.kind {
                        crate::definition::DefinitionKind::Dialect => UNRESOLVED_DIALECT,
                        crate::definition::DefinitionKind::Schema => UNRESOLVED_SCHEMA,
                    };
                    let record = record(code, &["context slot", name], ORIGIN);
                    self.diagnose(at_request(record, shown, request));
                    return None;
                }
            }
        }
        session.context().project(&slots).ok()
    }

    /// Execute one import request: resolve its target or targets, then
    /// invoke each under the request's policy.
    fn import(&mut self, request: &ImportRequest, shown: &str, context: Context) {
        let targets = self.targets(request);
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
    }
    if let Some(site) = site {
        site.relocate(from, to);
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
            request.node.source = source;
            Item::Import(request)
        }
        Item::PendingBinding(pending) => {
            let mut pending = pending.clone();
            pending.node.source = source;
            Item::PendingBinding(pending)
        }
        Item::Expansion(expansion) => {
            let mut expansion = expansion.clone();
            expansion.node.source = source;
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
