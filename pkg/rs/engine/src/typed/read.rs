//! The typed reader: binds documents of one definition, follows their
//! imports where written, and hands each record and declaration to the
//! processor's handler for its role and type.

use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};
use kdlith_model::{
    BoundContribution, BoundItem, Field, Identity, ImportRequest, NodeId, ScopeCompletion, SourceId,
};
use kdlith_runtime::{Admission, Context, InvocationId, Invocations, SessionError, SlotId};
use serde::Serialize;

use crate::bind::BindingSession;
use crate::bindings::Role;
use crate::definition::Definition;
use crate::diag::{IMPORT_UNRESOLVED, record};
use crate::documents::{import_failure, read_text};
use crate::typed::conform;
use crate::typed::shape::{Declarations, Handler};
use crate::typed::value::Fields;

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
    },
    /// A record, read as the processor's type for it.
    Record {
        node: NodeId,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
    },
    /// A declaration with its identity, read as the processor's type.
    Declaration {
        node: NodeId,
        identity: Identity,
        #[serde(rename = "type")]
        type_name: String,
        value: T,
    },
    Contribution(Box<BoundContribution>),
    /// An import as written; the imported document's items follow it.
    Import(ImportRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
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
}

impl<'d, T: 'static> TypedReader<'d, T> {
    /// A reader for `def`, after proving `declarations` agree with it.
    pub fn new(
        def: &'d Definition,
        declarations: &'static Declarations<T>,
    ) -> Result<Self, Vec<DiagnosticRecord>> {
        let disagreements = conform::run(def, declarations);
        if disagreements.is_empty() {
            return Ok(Self { def, declarations });
        }
        let processor = format!("{} {}", declarations.kind.root_name(), declarations.name);
        Err(disagreements
            .iter()
            .map(|d| mismatch(&processor, &d.subject, &d.reason))
            .collect())
    }

    /// Read every entry document and the documents they import, giving
    /// each item to `sink` as it is read. `Ok(true)` when any error
    /// diagnostic was among the items.
    pub fn read(
        &self,
        entries: &[&Path],
        sink: &mut dyn FnMut(TypedItem<T>),
    ) -> Result<bool, ReadError> {
        let mut reading = Reading {
            reader: self,
            invocations: Invocations::new(),
            sink,
            failed: false,
            broken: None,
        };
        let layout = self.def.plan.context.layout.clone();
        for entry in entries {
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
        Ok(reading.failed)
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
    sink: &'r mut dyn FnMut(TypedItem<T>),
    failed: bool,
    broken: Option<ReadError>,
}

impl<T: 'static> Reading<'_, '_, T> {
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
                Ok(text) => self.bind(id, &shown, &text, context.ok()),
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
        });
        let mut session = BindingSession::with_context(def, source, shown, text, context);
        while self.broken.is_none() {
            match session.next_item() {
                Ok(Some(item)) => {
                    if let BoundItem::Import(request) = &item {
                        let projected = self.projection(&session, request, shown);
                        (self.sink)(TypedItem::Import(request.clone()));
                        if let Some(context) = projected {
                            self.import(request, shown, context);
                        }
                        continue;
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
                match reader.value(Role::Record, &record.type_name, &record.fields) {
                    Ok(value) => TypedItem::Record {
                        node: record.node,
                        type_name: record.type_name,
                        value,
                    },
                    Err(diagnostic) => return self.diagnose(*diagnostic),
                }
            }
            BoundItem::Declaration(declaration) => {
                let fields = &declaration.fields;
                match reader.value(Role::Declaration, &declaration.type_name, fields) {
                    Ok(value) => TypedItem::Declaration {
                        node: declaration.node,
                        identity: declaration.identity,
                        type_name: declaration.type_name,
                        value,
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
                    let target =
                        format!("{} (projects undeclared slot {name})", request.expression);
                    let record = record(IMPORT_UNRESOLVED, &[&target], ORIGIN);
                    self.diagnose(at_request(record, shown, request));
                    return None;
                }
            }
        }
        session.context().project(&slots).ok()
    }

    fn import(&mut self, request: &ImportRequest, shown: &str, context: Context) {
        let admitted =
            self.invocations
                .request_projected(&request.expression, request.repeated, context);
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
