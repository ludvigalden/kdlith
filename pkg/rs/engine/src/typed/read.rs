//! The typed reader: reads documents of one definition through the
//! engine's data reader and hands each record and declaration to the
//! processor's handler for its role and type.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::{
    BoundContribution, ExpansionRequest, Field, Identity, ImportRequest, Item, Location, NodeId,
    PendingBinding, ResolvedContribution, ScopeCompletion, SourceId,
};
use kdlith_runtime::{Handlers, SessionError, SlotValue};
use serde::Serialize;

use crate::bindings::Role;
use crate::data::{DataReader, Entering, EntrySource, ReadOptions, Source};
use crate::definition::{CheckScope, Definition};
use crate::link::{
    DuplicateDeclaration, ImportAliases, Link, LinkInputs, LinkOutcome, LinkReport, link_reading,
};
use crate::resolution::SharedContributionResolver;
use crate::typed::conform;
use crate::typed::links::{ItemVariant, Ref};
use crate::typed::origin::Origin;
use crate::typed::shape::{Declarations, Handler};
use crate::typed::value::{Fields, FromValue, Mismatch};
use crate::value_resolver::SharedResolver;

const MISMATCH: &str = "KDLITH-GEN003";

/// One item of a typed reading, in stream order.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "item", content = "data", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TypedItem<T> {
    /// A document's items follow, up to its scope completion.
    Document {
        source: SourceId,
        path: String,
        /// The document's text, so a compiler consuming the stream can
        /// locate its diagnostics; not part of the item's serialized form.
        #[serde(skip)]
        text: String,
        /// The slots [`TypedReader::capturing`] named, in the order it
        /// named them, each with its value as the document's first node
        /// binds under it; `None` where the slot is unset. Empty unless
        /// capture was asked for.
        #[serde(skip)]
        context: Vec<(String, Option<SlotValue>)>,
    },
    /// A record, read as the processor's type for it.
    Record {
        node: NodeId,
        /// The declared node that bound the record, by name: several
        /// nodes may bind one type. Not part of the item's serialized form.
        #[serde(skip)]
        node_name: String,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
        /// The span of the node the record is; not part of the item's
        /// serialized form.
        #[serde(skip)]
        span: ByteSpan,
        /// Where the record was written, as the bound record carries it.
        #[serde(skip)]
        site: Option<Location>,
        /// The document the record was read from.
        #[serde(skip)]
        origin: Origin,
        /// The slots [`TypedReader::capturing`] named, in the order it
        /// named them, each with its value as the record binds under it;
        /// `None` where the slot is unset. Empty unless capture was asked
        /// for.
        #[serde(skip)]
        context: Vec<(String, Option<SlotValue>)>,
    },
    /// A declaration with its identity, read as the processor's type.
    Declaration {
        node: NodeId,
        /// The declared node that bound the declaration, by name. Not
        /// part of the item's serialized form.
        #[serde(skip)]
        node_name: String,
        identity: Identity,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
        /// The span of the node the declaration is; not part of the
        /// item's serialized form.
        #[serde(skip)]
        span: ByteSpan,
        /// Where the declaration was written, as the bound declaration
        /// carries it.
        #[serde(skip)]
        site: Option<Location>,
        /// The document the declaration was read from.
        #[serde(skip)]
        origin: Origin,
        /// The slots [`TypedReader::capturing`] named, as on a record.
        #[serde(skip)]
        context: Vec<(String, Option<SlotValue>)>,
    },
    Contribution(Box<BoundContribution>),
    /// A dependent value kept raw until its referenced declaration is
    /// known; never a validated value.
    PendingBinding(PendingBinding),
    /// An import as written; the imported document's items follow it.
    Import(ImportRequest),
    /// An include written among a definition's like members, as
    /// written: the request is the position marker the compiler's
    /// expansion pass splices at.
    Expansion(ExpansionRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
    /// A contribution target the registered contribution resolver
    /// settled, after the reading's documents.
    Resolution(ResolvedContribution),
}

impl<T> TypedItem<T> {
    /// A contribution's value, or a resolution's settled value, read as
    /// `C`: the processor's contribution type, or one type its
    /// contribution nodes declare. `None` for every other item.
    pub fn contribution<C: FromValue>(&self) -> Option<Result<C, Mismatch>> {
        let value = match self {
            TypedItem::Contribution(contribution) => &contribution.value,
            TypedItem::Resolution(resolved) => &resolved.value,
            _ => return None,
        };
        Some(C::from_value(value))
    }
}

/// Why a reading stopped short: never a document error, which is an
/// item of the stream, but a broken contract underneath it.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ReadError {
    /// A binding session broke its event-source or context contract.
    Session(SessionError),
    /// The binder yielded an item kind this reader does not know.
    UnsupportedItem,
    /// [`TypedReader::capturing`] named a slot the definition does not
    /// declare.
    UnknownSlot(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Session(error) => error.fmt(f),
            ReadError::UnsupportedItem => f.write_str("the binder yielded an unsupported item"),
            ReadError::UnknownSlot(name) => {
                write!(f, "the definition declares no context slot {name}")
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// Why a reference of a collected reading yields no value of the
/// requested type. A failed link carries the engine's outcome as the
/// link stage decided it; nothing is defaulted.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// The reading was not linked: no [`TypedReader::link`] step was
    /// requested.
    NotLinked,
    /// The reading linked no reference written where this one was: it
    /// came from another reading, or no declared reference type looks
    /// up its domain.
    NotLinkedHere,
    /// The reference did not resolve: unresolved (`KDLITH-REF001`),
    /// ambiguous (`KDLITH-REF003`), or failing a required qualifier
    /// (`KDLITH-REF004`), with the alias namespaces that hid a match.
    Link {
        outcome: LinkOutcome,
        gated: Vec<String>,
    },
    /// The one declaration it resolved to is not read as the requested
    /// type, or was not read at all because its typing failed.
    NotOfType,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotLinked => f.write_str("the reading was not linked"),
            ResolveError::NotLinkedHere => {
                f.write_str("the reading linked no reference written at this site")
            }
            ResolveError::Link { outcome, .. } => match outcome.code() {
                Some(code) => write!(f, "the reference did not resolve ({code})"),
                None => f.write_str("the reference did not resolve"),
            },
            ResolveError::NotOfType => {
                f.write_str("the declaration is not read as the requested type")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// A typed reading collected whole: its items in stream order, linked
/// when the reader asked for linking, so each reference its values hold
/// resolves through the link stage's outcome for that reference.
#[derive(Clone, Debug)]
pub struct Reading<T> {
    items: Vec<TypedItem<T>>,
    documents: BTreeMap<SourceId, String>,
    report: Option<LinkReport>,
    diagnostics: Vec<DiagnosticRecord>,
    failed: bool,
    /// Each linked reference's link, by the site it was written at: a
    /// site is one token of one document, so it names one reference.
    sites: HashMap<Location, (usize, usize)>,
    /// Each typed declaration's item, by node.
    declarations: HashMap<NodeId, usize>,
}

impl<T> Reading<T> {
    fn new(
        items: Vec<TypedItem<T>>,
        report: Option<LinkReport>,
        scope: Option<CheckScope>,
        failed: bool,
    ) -> Self {
        let mut documents = BTreeMap::new();
        let mut diagnostics = Vec::new();
        let mut declarations = HashMap::new();
        for (at, item) in items.iter().enumerate() {
            match item {
                TypedItem::Document { source, path, .. } => {
                    documents.insert(*source, path.clone());
                }
                TypedItem::Diagnostic(record) => diagnostics.push((**record).clone()),
                TypedItem::Declaration { node, .. } => {
                    declarations.insert(*node, at);
                }
                _ => {}
            }
        }
        let mut sites = HashMap::new();
        if let Some(report) = &report {
            diagnostics.extend(report.diagnostics.iter().cloned());
            for (kind, entry) in report.types.iter().enumerate() {
                for (at, link) in entry.links.iter().enumerate() {
                    sites.entry(link.reference.site).or_insert((kind, at));
                }
            }
        }
        let gated = scope == Some(CheckScope::Complete)
            && report.as_ref().is_some_and(LinkReport::has_errors);
        Self {
            items,
            documents,
            report,
            diagnostics,
            failed: failed || gated,
            sites,
            declarations,
        }
    }

    /// Every item of the reading, in stream order, pending typed
    /// bindings among them at their nodes' positions.
    pub fn items(&self) -> &[TypedItem<T>] {
        &self.items
    }

    /// The reading's items, in stream order.
    pub fn into_items(self) -> Vec<TypedItem<T>> {
        self.items
    }

    /// Each document's path as the reading showed it.
    pub fn documents(&self) -> &BTreeMap<SourceId, String> {
        &self.documents
    }

    /// The link stage's report, when the reading was linked.
    pub fn link_report(&self) -> Option<&LinkReport> {
        self.report.as_ref()
    }

    /// The declarations that repeat an earlier one's identity
    /// (`KDLITH-REF002`); empty when the reading was not linked. The
    /// first declaration stays the one references resolve to.
    pub fn duplicates(&self) -> &[DuplicateDeclaration] {
        self.report
            .as_ref()
            .map_or(&[], |report| report.duplicates.as_slice())
    }

    /// The stream's diagnostics in stream order, then the link stage's:
    /// duplicates, failed links, and pending payloads failing their
    /// member type (`KDLITH-REF005`).
    pub fn diagnostics(&self) -> &[DiagnosticRecord] {
        &self.diagnostics
    }

    /// Whether the reading failed: an error among its items, or, linked
    /// under complete scope, any link diagnostic. Linked under local
    /// scope, the link stage reports without deciding this.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The link the reading's link stage made for `reference`, found by
    /// the site it was written at. Its `gated` lists the alias
    /// namespaces that hid a match; `resolve` reports them only on
    /// failure, so a resolved reference's gate list is read here.
    pub fn link_of<V>(&self, reference: &Ref<V>) -> Option<&Link> {
        let (kind, at) = *self.sites.get(&reference.site())?;
        self.report.as_ref()?.types.get(kind)?.links.get(at)
    }

    /// The declaration `reference` resolved to, read as its target type.
    pub fn resolve<D>(&self, reference: &Ref<D>) -> Result<&D, ResolveError>
    where
        T: ItemVariant<D>,
    {
        self.settled::<D, D>(reference)
    }

    /// The declaration `reference` resolved to, read as `D`: the link
    /// stage decides the declaration, and the processor's item type
    /// projects it.
    pub fn settled<D, V>(&self, reference: &Ref<V>) -> Result<&D, ResolveError>
    where
        T: ItemVariant<D>,
    {
        if self.report.is_none() {
            return Err(ResolveError::NotLinked);
        }
        let link = self.link_of(reference).ok_or(ResolveError::NotLinkedHere)?;
        let target = match (&link.outcome, link.candidates.as_slice()) {
            (LinkOutcome::Resolved, [target]) => target,
            (outcome, _) => {
                return Err(ResolveError::Link {
                    outcome: outcome.clone(),
                    gated: link.gated.clone(),
                });
            }
        };
        let at = self
            .declarations
            .get(&target.node)
            .ok_or(ResolveError::NotOfType)?;
        match &self.items[*at] {
            TypedItem::Declaration { value, .. } => value.variant().ok_or(ResolveError::NotOfType),
            _ => Err(ResolveError::NotOfType),
        }
    }
}

/// A typed processor's reader over one compiled definition: the
/// engine's data reader decides what is read — imports, profiles,
/// handlers, output policy — and this reader types each record and
/// declaration through the processor's handler for it.
pub struct TypedReader<'d, T: 'static> {
    declarations: &'static Declarations<T>,
    reader: DataReader<'d>,
    options: ReadOptions,
    /// The scope the reading links under, when linking was asked for.
    linking: Option<CheckScope>,
    /// The context slots each document item carries, by name, in the
    /// order they were asked for.
    capture: Vec<String>,
}

impl<'d, T: 'static> TypedReader<'d, T> {
    /// A reader for `def`, after proving `declarations` agree with it.
    pub fn new(
        def: &'d Definition,
        declarations: &'static Declarations<T>,
    ) -> Result<Self, Vec<DiagnosticRecord>> {
        let disagreements = conform::run(def, declarations);
        if disagreements.is_empty() {
            return Ok(Self {
                declarations,
                reader: DataReader::new(def),
                options: ReadOptions::new(),
                linking: None,
                capture: Vec::new(),
            });
        }
        let processor = format!("{} {}", declarations.kind.root_name(), declarations.name);
        Err(disagreements
            .iter()
            .map(|d| mismatch(&processor, &d.subject, &d.reason))
            .collect())
    }

    /// Supply the discovery anchors the documents' import expressions
    /// expand against, under the grammar the documents themselves
    /// declare, in place of any supplied before. A declared anchor left
    /// unsupplied, or a supplied one no grammar declares, fails closed
    /// (`KDLITH-IMP005`).
    #[must_use]
    pub fn with_anchors(mut self, anchors: &[(String, PathBuf)]) -> Self {
        self.options.set_anchors(anchors);
        self
    }

    /// Read under `options`, in place of every option set before: the
    /// profile or profile set, the anchors, and the output policy, as
    /// [`DataReader::read`] applies them.
    #[must_use]
    pub fn with_options(mut self, options: ReadOptions) -> Self {
        self.options = options;
        self
    }

    /// Name the profile every entry document binds under, as
    /// [`ReadOptions::with_profile`] does.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.options = self.options.with_profile(profile);
        self
    }

    /// Install the context handlers a handler effect executes through;
    /// one the registry lacks fails at its effect site.
    #[must_use]
    pub fn with_handlers(mut self, handlers: Handlers) -> Self {
        self.reader = self.reader.replace_handlers(handlers);
        self
    }

    /// Register the consuming program's value resolver: the static hook
    /// through which secret values and context variables a schema
    /// declares as computed defaults are resolved. The resolver receives
    /// the call's context information and the declared descriptors as
    /// data; a value it declines fails its node (`KDLITH-BND009`).
    #[must_use]
    pub fn with_resolver(mut self, resolver: SharedResolver) -> Self {
        self.reader = self.reader.with_resolver(resolver);
        self
    }

    /// Register the program's contribution resolver: once a full
    /// reading's documents are read without error, and the schema
    /// declares a resolution policy, every contribution record is
    /// routed to it and each target it settles follows the documents'
    /// items as a resolution item. A policy naming another resolver, or
    /// a refusal, fails the reading (`KDLITH-DLC006`, `KDLITH-DLC010`).
    #[must_use]
    pub fn with_contribution_resolver(mut self, resolver: SharedContributionResolver) -> Self {
        self.reader = self.reader.with_contribution_resolver(resolver);
        self
    }

    /// Link every reading this reader makes, as `vet data` links one:
    /// once the documents are read, the link stage resolves each
    /// reference a declared reference type looks up and validates each
    /// pending payload. Under complete scope a link diagnostic fails the
    /// reading; under local scope the stage reports without deciding
    /// that. A reading that stops short of its documents is not linked.
    #[must_use]
    pub fn link(mut self, scope: CheckScope) -> Self {
        self.linking = Some(scope);
        self
    }

    /// Carry, on every document item, the named context slots' values as
    /// the document's first node binds under them, and on every record
    /// and declaration their values as it binds, in the order named, in
    /// place of any named before. A name the definition declares no slot
    /// for fails the reading before any document is read
    /// ([`ReadError::UnknownSlot`]).
    #[must_use]
    pub fn capturing(mut self, slots: &[&str]) -> Self {
        self.capture = slots.iter().map(|&slot| slot.to_owned()).collect();
        self
    }

    /// Read every entry document and the documents they import, giving
    /// each item to `sink` as it is read. `Ok(true)` when any error
    /// diagnostic was among the items, or, linked under complete scope,
    /// the link stage reported any.
    pub fn read(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
    ) -> Result<bool, ReadError> {
        self.read_scoped(entries, sink, true)
            .map(|(failed, report)| failed || self.gates(report.as_ref()))
    }

    /// Read the named entry documents only, giving each import request
    /// as an item without following it: local checking reads named
    /// documents only.
    pub fn read_local(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
    ) -> Result<bool, ReadError> {
        self.read_scoped(entries, sink, false)
            .map(|(failed, report)| failed || self.gates(report.as_ref()))
    }

    /// Read as [`TypedReader::read`] does and keep the whole reading:
    /// its items in stream order and, when linking was asked for, the
    /// link stage's report its references resolve through.
    pub fn collect(&self, entries: &[&Path]) -> Result<Reading<T>, ReadError> {
        let mut items = Vec::new();
        let (failed, report) = self.read_scoped(entries, &mut |item| items.push(item), true)?;
        Ok(Reading::new(items, report, self.linking, failed))
    }

    /// Read as [`TypedReader::read`] does, over documents held in
    /// memory instead of read from disk (see [`Source`]): the engine
    /// reading decides everything, this reader only types its items.
    /// An import inside an in-memory document fails closed
    /// (`KDLITH-IMP001`).
    pub fn read_sources(
        &self,
        sources: &[Source],
        sink: &mut dyn FnMut(TypedItem<T>),
    ) -> Result<bool, ReadError> {
        self.sources_scoped(sources, sink, true)
            .map(|(failed, report)| failed || self.gates(report.as_ref()))
    }

    /// Read as [`TypedReader::collect`] does, over documents held in
    /// memory instead of read from disk.
    pub fn collect_sources(&self, sources: &[Source]) -> Result<Reading<T>, ReadError> {
        let mut items = Vec::new();
        let (failed, report) = self.sources_scoped(sources, &mut |item| items.push(item), true)?;
        Ok(Reading::new(items, report, self.linking, failed))
    }

    /// Whether a reading's link report fails it: under complete scope
    /// only.
    fn gates(&self, report: Option<&LinkReport>) -> bool {
        self.linking == Some(CheckScope::Complete) && report.is_some_and(LinkReport::has_errors)
    }

    fn read_scoped(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
        follow: bool,
    ) -> Result<(bool, Option<LinkReport>), ReadError> {
        let entries: Vec<EntrySource> = entries
            .iter()
            .map(|entry| EntrySource::Path(entry))
            .collect();
        self.read_over(&entries, sink, follow)
    }

    fn sources_scoped(
        &self,
        sources: &[Source],
        sink: &mut dyn FnMut(TypedItem<T>),
        follow: bool,
    ) -> Result<(bool, Option<LinkReport>), ReadError> {
        let entries: Vec<EntrySource> = sources
            .iter()
            .map(|source| EntrySource::Memory {
                shown: &source.shown,
                text: &source.text,
            })
            .collect();
        self.read_over(&entries, sink, follow)
    }

    fn read_over(
        &self,
        entries: &[EntrySource<'_>],
        sink: &mut dyn FnMut(TypedItem<T>),
        follow: bool,
    ) -> Result<(bool, Option<LinkReport>), ReadError> {
        let captured = self.captured_slots()?;
        // The data reader gives each document's text, and its entry
        // context when asked, just before its document item, and each
        // record's or declaration's context just before it, so they pair
        // in stream order.
        let text = Cell::new(String::new());
        let mut texts = |read: &str| text.set(read.to_owned());
        let context = RefCell::new(Vec::new());
        let mut entered = |values: &[Option<SlotValue>]| {
            *context.borrow_mut() = captured
                .iter()
                .map(|(name, at)| (name.clone(), values.get(*at).cloned().flatten()))
                .collect();
        };
        let entering: Option<Entering<'_>> = (!captured.is_empty()).then_some(&mut entered);
        let mut origins: HashMap<SourceId, Origin> = HashMap::new();
        let mut imports = ImportAliases::default();
        let mut mismatched = false;
        let mut unsupported = false;
        let mut inputs = self.linking.map(|_| LinkInputs::new());
        let mut typing = |item: Item| {
            if unsupported {
                return;
            }
            // The link stage sees the engine's items, as `vet data`
            // links them, whatever the processor makes of them.
            if let Some(inputs) = &mut inputs {
                inputs.observe(&item);
            }
            // An item's origin is its document's, found by the source its
            // node is in: an importer's items after a nested import's
            // document stay the importer's.
            let alias = imports.observe(&item);
            if let Item::Document(document) = &item {
                let origin = Origin {
                    path: document.path.clone(),
                    alias,
                };
                origins.insert(document.source, origin);
            }
            let typed = match self.typed(item, &text, &context, &origins) {
                // A reading a record failed to type is incomplete, so no
                // target is settled from its contributions.
                Some(Ok(TypedItem::Resolution(_))) if mismatched => return,
                Some(Ok(typed)) => typed,
                Some(Err(diagnostic)) => {
                    mismatched = true;
                    TypedItem::Diagnostic(diagnostic)
                }
                // An item outside this reader's vocabulary is never
                // silently dropped.
                None => {
                    unsupported = true;
                    return;
                }
            };
            sink(typed);
        };
        let failed = self.reader.read_documents(
            entries,
            &self.options,
            &mut typing,
            &mut texts,
            entering,
            follow,
        )?;
        if unsupported {
            return Err(ReadError::UnsupportedItem);
        }
        let report = inputs.map(|inputs| link_reading(self.reader.definition(), &inputs));
        Ok((failed || mismatched, report))
    }

    /// Each slot [`TypedReader::capturing`] named, with its position in
    /// the definition's slot order; a name no slot carries fails closed.
    fn captured_slots(&self) -> Result<Vec<(String, usize)>, ReadError> {
        let def = self.reader.definition();
        self.capture
            .iter()
            .map(|name| {
                def.names
                    .get(name)
                    .and_then(|sym| def.plan.context.slot(sym))
                    .map(|slot| (name.clone(), slot.index()))
                    .ok_or_else(|| ReadError::UnknownSlot(name.clone()))
            })
            .collect()
    }

    /// One engine item as the processor reads it: a record or
    /// declaration through its handler, every other item as its twin;
    /// `None` for an item this reader does not know.
    fn typed(
        &self,
        item: Item,
        text: &Cell<String>,
        context: &RefCell<Vec<(String, Option<SlotValue>)>>,
        origins: &HashMap<SourceId, Origin>,
    ) -> Option<Result<TypedItem<T>, Box<DiagnosticRecord>>> {
        // Every record and declaration follows its document's item.
        let origin = |node: NodeId| origins.get(&node.source).cloned().unwrap_or_default();
        let typed = match item {
            Item::Document(document) => TypedItem::Document {
                source: document.source,
                path: document.path,
                text: text.take(),
                context: context.take(),
            },
            Item::Record(record) => {
                let context = context.take();
                let value = self.value(
                    Role::Record,
                    &record.node_name,
                    &record.type_name,
                    &record.fields,
                );
                return Some(value.map(|value| TypedItem::Record {
                    node: record.node,
                    node_name: record.node_name,
                    type_name: record.type_name,
                    value,
                    span: record.span,
                    site: record.site,
                    origin: origin(record.node),
                    context,
                }));
            }
            Item::Declaration(declaration) => {
                let context = context.take();
                let value = self.value(
                    Role::Declaration,
                    &declaration.node_name,
                    &declaration.type_name,
                    &declaration.fields,
                );
                return Some(value.map(|value| TypedItem::Declaration {
                    node: declaration.node,
                    node_name: declaration.node_name,
                    identity: declaration.identity,
                    type_name: declaration.type_name,
                    value,
                    span: declaration.span,
                    site: declaration.site,
                    origin: origin(declaration.node),
                    context,
                }));
            }
            Item::Contribution(contribution) => TypedItem::Contribution(Box::new(contribution)),
            Item::Import(request) => TypedItem::Import(request),
            Item::Expansion(request) => TypedItem::Expansion(request),
            Item::PendingBinding(pending) => TypedItem::PendingBinding(pending),
            Item::Diagnostic(record) => TypedItem::Diagnostic(record),
            Item::ScopeComplete(completion) => TypedItem::ScopeComplete(completion),
            Item::Resolution(resolved) => TypedItem::Resolution(resolved),
            _ => return None,
        };
        Some(Ok(typed))
    }

    /// The handler registered for the item's declaring node, else the
    /// one registered for its type.
    fn handler(&self, role: Role, node: &str, type_name: &str) -> Option<&'static Handler<T>> {
        let handlers = || {
            self.declarations
                .handlers
                .iter()
                .flat_map(|handlers| handlers.iter())
                .filter(move |handler| handler.role == role && handler.ty == type_name)
        };
        handlers()
            .find(|handler| handler.node == Some(node))
            .or_else(|| handlers().find(|handler| handler.node.is_none()))
    }

    /// Read a bound item's fields with the handler for its role, its
    /// declaring node, and its type.
    fn value(
        &self,
        role: Role,
        node: &str,
        type_name: &str,
        fields: &[Field],
    ) -> Result<T, Box<DiagnosticRecord>> {
        let processor = format!(
            "{} {}",
            self.declarations.kind.root_name(),
            self.declarations.name
        );
        let subject = format!("{} items of type {type_name}", role.name());
        let Some(handler) = self.handler(role, node, type_name) else {
            return Err(Box::new(mismatch(&processor, &subject, "no handler")));
        };
        (handler.read)(Fields::of_item(fields))
            .map_err(|m| Box::new(mismatch(&processor, &subject, &m.to_string())))
    }
}

fn mismatch(processor: &str, subject: &str, reason: &str) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(MISMATCH)
        .expect("the processor mismatch code is registered and never retired");
    let record = match lookup(MISMATCH) {
        Some(definition) => record.with_message(render(
            definition.message_template,
            &[processor, subject, reason],
        )),
        None => record,
    };
    record.with_origin("processor")
}
