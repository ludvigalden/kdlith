//! The typed reader: binds documents of one definition, follows their
//! imports where written, and hands each record and declaration to the
//! processor's handler for its role and type.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::{
    BoundContribution, BoundItem, Field, Identity, ImportRequest, NodeId, ResolvedContribution,
    ScopeCompletion, SourceId,
};
use kdlith_runtime::{
    Admission, Context, ContributionRecord, InvocationId, Invocations, SessionError, SlotId,
};
use serde::Serialize;

use crate::anchors::{self, Base, DeclaredGrammars};
use crate::bind::BindingSession;
use crate::bindings::Role;
use crate::definition::{Definition, DefinitionKind, ProfileSelection};
use crate::diag::{UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA, record};
use crate::discovery;
use crate::documents::{import_failure, read_text};
use crate::resolution::{SharedContributionResolver, failure_record, resolve};
use crate::typed::conform;
use crate::typed::shape::{Declarations, Handler};
use crate::typed::value::Fields;
use crate::value_resolver::SharedResolver;

const MISMATCH: &str = "KDLITH-GEN003";
const ORIGIN: &str = "data-document";

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
    },
    /// A record, read as the processor's type for it.
    Record {
        node: NodeId,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
        /// The span of the node the record is; not part of the item's
        /// serialized form.
        #[serde(skip)]
        span: ByteSpan,
    },
    /// A declaration with its identity, read as the processor's type.
    Declaration {
        node: NodeId,
        identity: Identity,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
        /// The span of the node the declaration is; not part of the
        /// item's serialized form.
        #[serde(skip)]
        span: ByteSpan,
    },
    Contribution(Box<BoundContribution>),
    /// An import as written; the imported document's items follow it.
    Import(ImportRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
    /// A contribution target the registered contribution resolver
    /// settled, after the reading's documents.
    Resolution(ResolvedContribution),
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
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Session(error) => error.fmt(f),
            ReadError::UnsupportedItem => f.write_str("the binder yielded an unsupported item"),
        }
    }
}

impl std::error::Error for ReadError {}

/// A typed processor's reader over one compiled definition.
pub struct TypedReader<'d, T: 'static> {
    def: &'d Definition,
    declarations: &'static Declarations<T>,
    /// The host-supplied discovery anchors the documents' own import
    /// expressions expand against.
    anchors: Vec<(String, PathBuf)>,
    /// The value resolver the consuming program registered: an absent
    /// field whose declared fallback is computed resolves through it.
    resolver: Option<SharedResolver>,
    /// The contribution resolver the program registered: a full reading
    /// routes every contribution record to it under the schema's
    /// declared policy.
    contributions: Option<SharedContributionResolver>,
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
                def,
                declarations,
                anchors: Vec::new(),
                resolver: None,
                contributions: None,
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
    /// declare. A declared anchor left unsupplied, or a supplied one no
    /// grammar read declares, fails closed (`KDLITH-IMP005`).
    #[must_use]
    pub fn with_anchors(mut self, anchors: &[(String, PathBuf)]) -> Self {
        self.anchors = anchors.to_vec();
        self
    }

    /// Register the consuming program's value resolver: the static hook
    /// through which secret values and context variables a schema
    /// declares as computed defaults are resolved. The resolver receives
    /// the call's context information and the declared descriptors as
    /// data; a value it declines fails its node (`KDLITH-BND009`).
    #[must_use]
    pub fn with_resolver(mut self, resolver: SharedResolver) -> Self {
        self.resolver = Some(resolver);
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
        self.contributions = Some(resolver);
        self
    }

    /// Read every entry document and the documents they import, giving
    /// each item to `sink` as it is read. `Ok(true)` when any error
    /// diagnostic was among the items.
    pub fn read(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
    ) -> Result<bool, ReadError> {
        self.read_scoped(entries, sink, true)
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
    }

    fn read_scoped(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
        follow: bool,
    ) -> Result<bool, ReadError> {
        // A local reading leaves imports unfollowed, so its records are
        // never the whole picture a resolver must see.
        let resolver = self
            .contributions
            .as_ref()
            .filter(|_| follow && self.def.resolution.is_some());
        let mut records: Vec<ContributionRecord> = Vec::new();
        let mut tap = |item: TypedItem<T>| {
            if resolver.is_some()
                && let TypedItem::Contribution(contribution) = &item
            {
                records.push(ContributionRecord::from_bound(contribution));
            }
            sink(item);
        };
        let mut reading = Reading {
            reader: self,
            invocations: Invocations::new(),
            grammars: DeclaredGrammars::compiled(self.def),
            selection: ProfileSelection::Unprofiled,
            sink: &mut tap,
            failed: false,
            broken: None,
            follow,
        };
        if reading.declared() {
            // Every entry declares its grammars before any import runs,
            // as the compiler validates expressions against the full set.
            reading.grammars = DeclaredGrammars::default();
            for entry in entries {
                if let Ok(text) = std::fs::read_to_string(entry) {
                    reading.grammars.scan(&text);
                }
            }
        }
        let layout = self.def.plan.context.layout.clone();
        for entry in entries {
            // One selection per entry document, from its source path as
            // the reading spells it; its imports bind under it.
            let shown = entry.display().to_string();
            reading.selection = self.def.select_profile(None, &[], &shown);
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
        let grammars = &reading.grammars;
        for record in
            anchors::undeclared(&self.anchors, &grammars.grammars, &grammars.names, ORIGIN)
        {
            reading.diagnose(record);
        }
        let mut failed = reading.failed;
        // A failed reading's records are incomplete; resolving them
        // would report on a partial picture.
        if let Some(resolver) = resolver.filter(|_| !failed && !records.is_empty()) {
            match resolve(self.def, resolver.as_ref(), &records) {
                Ok(resolved) => resolved
                    .into_iter()
                    .for_each(|resolved| sink(TypedItem::Resolution(resolved))),
                Err(error) => {
                    failed = true;
                    let record = failure_record(&error, ORIGIN);
                    sink(TypedItem::Diagnostic(Box::new(record)));
                }
            }
        }
        Ok(failed)
    }

    fn handler(&self, role: Role, type_name: &str) -> Option<&'static Handler<T>> {
        self.declarations
            .handlers
            .iter()
            .flat_map(|handlers| handlers.iter())
            .find(|handler| handler.role == role && handler.ty == type_name)
    }

    /// Read a bound item's fields with the handler for its role and type.
    fn value(
        &self,
        role: Role,
        type_name: &str,
        fields: &[Field],
    ) -> Result<T, Box<DiagnosticRecord>> {
        let processor = format!(
            "{} {}",
            self.declarations.kind.root_name(),
            self.declarations.name
        );
        let subject = format!("{} items of type {type_name}", role.name());
        let Some(handler) = self.handler(role, type_name) else {
            return Err(Box::new(mismatch(&processor, &subject, "no handler")));
        };
        (handler.read)(Fields::of_item(fields))
            .map_err(|m| Box::new(mismatch(&processor, &subject, &m.to_string())))
    }
}

/// One reading in progress: the invocation stack and the sink.
struct Reading<'r, 'd, T: 'static> {
    reader: &'r TypedReader<'d, T>,
    invocations: Invocations,
    /// The discovery grammars the documents read so far declare.
    grammars: DeclaredGrammars,
    /// The profile selection of the entry document being read.
    selection: ProfileSelection,
    sink: &'r mut dyn FnMut(TypedItem<T>),
    failed: bool,
    broken: Option<ReadError>,
    /// Whether import requests are followed; a local reading gives each
    /// request as an item and stops there.
    follow: bool,
}

impl<T: 'static> Reading<'_, '_, T> {
    /// Whether the documents are definitions declaring their own
    /// grammars, rather than data documents of a compiled schema.
    fn declared(&self) -> bool {
        self.reader.def.kind == DefinitionKind::Dialect
    }

    fn diagnose(&mut self, record: DiagnosticRecord) {
        self.failed = true;
        (self.sink)(TypedItem::Diagnostic(Box::new(record)));
    }

    /// Bind the innermost active invocation's document, following each
    /// import as its request is read, then leave it.
    fn visit(&mut self, id: InvocationId) {
        if let Some(invocation) = self.invocations.get(id) {
            let (path, shown) = (invocation.path.clone(), invocation.shown.clone());
            let context = invocation.context.project(&self.all_slots());
            match read_text(&path, &shown, "document size", ORIGIN) {
                Ok(text) => {
                    if self.declared() {
                        self.grammars.scan(&text);
                    }
                    self.bind(id, &shown, &text, context.ok());
                }
                Err(record) => self.diagnose(*record),
            }
        }
        self.invocations.leave();
    }

    fn bind(&mut self, id: InvocationId, shown: &str, text: &str, context: Option<Context>) {
        let def = self.reader.def;
        let source = SourceId(u32::try_from(id.index()).unwrap_or(u32::MAX));
        let context = context.unwrap_or_else(|| Context::new(def.plan.context.layout.clone()));
        (self.sink)(TypedItem::Document {
            source,
            path: shown.to_owned(),
            text: text.to_owned(),
        });
        let selection = self.selection.clone();
        let mut session =
            BindingSession::with_selection(def, source, shown, text, context, selection)
                .with_resolver(self.reader.resolver.clone());
        while self.broken.is_none() {
            match session.next_item() {
                Ok(Some(item)) => {
                    if let BoundItem::Import(request) = &item {
                        if self.follow {
                            let projected = self.projection(&session, request, shown);
                            (self.sink)(TypedItem::Import(request.clone()));
                            if let Some(context) = projected {
                                self.import(request, shown, context);
                            }
                            continue;
                        }
                    }
                    self.item(item);
                }
                Ok(None) => break,
                Err(error) => self.broken = Some(ReadError::Session(error)),
            }
        }
        self.failed |= session.failed();
    }

    fn item(&mut self, item: BoundItem) {
        let reader = self.reader;
        let typed = match item {
            BoundItem::Record(record) => {
                let span = record.span;
                match reader.value(Role::Record, &record.type_name, &record.fields) {
                    Ok(value) => TypedItem::Record {
                        node: record.node,
                        type_name: record.type_name,
                        value,
                        span,
                    },
                    Err(diagnostic) => return self.diagnose(*diagnostic),
                }
            }
            BoundItem::Declaration(declaration) => {
                let span = declaration.span;
                let fields = declaration.fields;
                match reader.value(Role::Declaration, &declaration.type_name, &fields) {
                    Ok(value) => TypedItem::Declaration {
                        node: declaration.node,
                        identity: declaration.identity,
                        type_name: declaration.type_name,
                        value,
                        span,
                    },
                    Err(diagnostic) => return self.diagnose(*diagnostic),
                }
            }
            BoundItem::Contribution(contribution) => TypedItem::Contribution(contribution),
            BoundItem::Import(request) => TypedItem::Import(request),
            BoundItem::Diagnostic(record) => {
                self.failed = true;
                TypedItem::Diagnostic(record)
            }
            BoundItem::ScopeComplete(completion) => TypedItem::ScopeComplete(completion),
            // A bound item outside this reader's vocabulary is never
            // silently dropped.
            _ => {
                self.broken = Some(ReadError::UnsupportedItem);
                return;
            }
        };
        (self.sink)(typed);
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
                    // projection: by the kind of the importing definition.
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

    fn import(&mut self, request: &ImportRequest, shown: &str, context: Context) {
        if discovery::is_expression(&request.expression) {
            return self.import_expression(request, shown, context);
        }
        let admitted =
            self.invocations
                .request_projected(&request.expression, request.repeated, context);
        self.admitted(admitted, request, shown);
    }

    /// Expand a grammar-admitted expression under the grammar the
    /// documents declare, against the host-supplied anchors, and invoke
    /// each match in order under the request's policy.
    fn import_expression(&mut self, request: &ImportRequest, shown: &str, context: Context) {
        let Some(importer) = self
            .invocations
            .innermost()
            .and_then(|id| self.invocations.get(id))
        else {
            return;
        };
        let dir = importer.path.parent().unwrap_or(Path::new("")).to_owned();
        let shown_dir = Path::new(&importer.shown)
            .parent()
            .unwrap_or(Path::new(""))
            .to_owned();
        let names = &self.grammars.names;
        let anchors = anchors::anchor_map(&self.reader.anchors, names);
        let base = Base {
            dir: &dir,
            shown: &shown_dir,
        };
        let grammar = self.grammars.grammar();
        let targets = anchors::expression_targets(
            &request.expression,
            grammar,
            names,
            &anchors,
            &base,
            ORIGIN,
        );
        let targets = match targets {
            Ok(targets) => targets,
            Err(record) => return self.diagnose(at_request(*record, shown, request)),
        };
        for target in targets {
            let admitted = self.invocations.invoke_resolved(
                &target.resolved,
                &target.shown,
                request.repeated,
                context.clone(),
            );
            self.admitted(admitted, request, shown);
        }
    }

    /// Visit an admitted invocation, or report why it was refused.
    fn admitted(
        &mut self,
        admitted: Result<Admission, kdlith_runtime::ImportFailure>,
        request: &ImportRequest,
        shown: &str,
    ) {
        match admitted {
            Ok(Admission::Invoke(id)) => self.visit(id),
            Ok(_) => {}
            Err(failure) => {
                let record = import_failure(failure, ORIGIN);
                self.diagnose(at_request(record, shown, request));
            }
        }
    }

    /// Every slot of the definition: an invocation's own context is
    /// already its caller's projection.
    fn all_slots(&self) -> Vec<SlotId> {
        self.reader.def.plan.context.layout.slots().collect()
    }
}

fn at_request(record: DiagnosticRecord, shown: &str, request: &ImportRequest) -> DiagnosticRecord {
    let start = request.site.span.start;
    record.with_source(shown).with_span(
        request.site.span.byte_span(),
        Some(start.line),
        Some(start.column),
    )
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
