//! Loading definition documents and following their imports through the
//! runtime's invocation stack; every document reached contributes to the
//! definition, in visit order. Local checking reads named documents only.

use std::path::Path;

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::Value;
use kdlith_runtime::{
    Admission, Context, ContextLayout, ImportFailure, InvocationId, Invocations, Repeated, SlotId,
};

use crate::compile::{compile_dialect_scoped, compile_schema_scoped};
use crate::deferred::is_discovery_path;
use crate::definition::{CheckScope, Definition, DefinitionKind};
use crate::diag::{IMPORT_UNRESOLVED, UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA, located, record};
use crate::documents::{MAX_DOCUMENT_BYTES, import_failure, read_text};
use crate::extensions::Extensions;
use crate::names::Names;
use crate::plan::ContextPlan;
use crate::syntax::{ParsedDocument, SyntaxNode};

/// A definition document larger than this is refused before it is read.
pub const MAX_DEFINITION_BYTES: u64 = MAX_DOCUMENT_BYTES;

/// Load a dialect definition from its entry documents and compile it.
pub fn load_dialect(
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let documents = load(DefinitionKind::Dialect, entries, None, scope)?;
    compile_dialect_scoped(&documents, extensions, scope)
}

/// Load a schema definition from its entry documents and compile it under
/// its governing dialect, whose context slots its imports may project.
pub fn load_schema(
    dialect: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let documents = load(DefinitionKind::Schema, entries, Some(dialect), scope)?;
    compile_schema_scoped(dialect, &documents, extensions, scope)
}

/// One `import` node of a document's root, as far as resolution needs it.
/// A malformed request is skipped here; the compiler reports it.
struct Request {
    path: String,
    repeated: Repeated,
    project: Vec<String>,
    span: ByteSpan,
}

struct Loader<'g> {
    kind: DefinitionKind,
    scope: CheckScope,
    /// The context slots of the documents' describing dialect; a dialect
    /// being loaded describes itself, so its own documents get none.
    names: Option<&'g Names>,
    context: Option<&'g ContextPlan>,
    invocations: Invocations,
    documents: Vec<ParsedDocument>,
    records: Vec<DiagnosticRecord>,
}

fn load(
    kind: DefinitionKind,
    entries: &[&Path],
    governing: Option<&Definition>,
    scope: CheckScope,
) -> Result<Vec<ParsedDocument>, Vec<DiagnosticRecord>> {
    let mut loader = Loader {
        kind,
        scope,
        names: governing.map(|g| &g.names),
        context: governing.map(|g| &g.plan.context),
        invocations: Invocations::new(),
        documents: Vec::new(),
        records: Vec::new(),
    };
    let layout = loader
        .context
        .map_or_else(ContextLayout::new, |plan| plan.layout.clone());
    for entry in entries {
        match loader
            .invocations
            .enter_root(entry, Context::new(layout.clone()))
        {
            Ok(id) => loader.visit(id),
            Err(failure) => {
                let record = loader.failure(failure);
                loader.records.push(record);
            }
        }
    }
    if loader.records.is_empty() {
        Ok(loader.documents)
    } else {
        Err(loader.records)
    }
}

impl Loader<'_> {
    /// Read the innermost active invocation's document, then follow its
    /// imports in written order, then leave it.
    fn visit(&mut self, id: InvocationId) {
        if let Some(document) = self.read(id) {
            let requests = if self.scope == CheckScope::Complete {
                requests(self.kind, &document)
            } else {
                Vec::new()
            };
            let at = self.documents.len();
            self.documents.push(document);
            for request in requests {
                self.import(at, &request);
            }
        }
        self.invocations.leave();
    }

    fn import(&mut self, importer: usize, request: &Request) {
        let Some(project) = self.projection(importer, request) else {
            return;
        };
        match self
            .invocations
            .request(&request.path, request.repeated, &project)
        {
            Ok(Admission::Invoke(id)) => self.visit(id),
            Ok(Admission::Reuse(_)) => {}
            Ok(_) => {
                let record = record(IMPORT_UNRESOLVED, &[&request.path], self.kind.origin());
                let record = located(record, &self.documents[importer], request.span);
                self.records.push(record);
            }
            Err(failure) => {
                let record = self.failure(failure);
                let record = located(record, &self.documents[importer], request.span);
                self.records.push(record);
            }
        }
    }

    /// The projected slots, each declared by the describing dialect.
    fn projection(&mut self, importer: usize, request: &Request) -> Option<Vec<SlotId>> {
        let mut slots = Vec::new();
        for name in &request.project {
            let slot = self
                .names
                .and_then(|names| names.get(name))
                .and_then(|sym| self.context.and_then(|plan| plan.slot(sym)));
            match slot {
                Some(slot) => slots.push(slot),
                None => {
                    let code = match self.kind {
                        DefinitionKind::Dialect => UNRESOLVED_DIALECT,
                        DefinitionKind::Schema => UNRESOLVED_SCHEMA,
                    };
                    let record = record(code, &["context slot", name], self.kind.origin());
                    let record = located(record, &self.documents[importer], request.span);
                    self.records.push(record);
                    return None;
                }
            }
        }
        Some(slots)
    }

    fn read(&mut self, id: InvocationId) -> Option<ParsedDocument> {
        let invocation = self.invocations.get(id)?;
        let (path, shown) = (invocation.path.clone(), invocation.shown.clone());
        let text = match read_text(&path, &shown, "definition size", self.kind.origin()) {
            Ok(text) => text,
            Err(record) => {
                self.records.push(*record);
                return None;
            }
        };
        match ParsedDocument::parse(shown, &text) {
            Ok(document) => Some(document),
            Err(error) => {
                self.records.push(*error);
                None
            }
        }
    }

    fn failure(&self, failure: ImportFailure) -> DiagnosticRecord {
        import_failure(failure, self.kind.origin())
    }
}

/// The import requests of a document's definition roots, in written order.
fn requests(kind: DefinitionKind, document: &ParsedDocument) -> Vec<Request> {
    document
        .nodes()
        .iter()
        .filter(|root| root.name == kind.root_name())
        .flat_map(|root| root.children.iter())
        .filter(|node| node.name == "import")
        .filter_map(request)
        .collect()
}

fn request(node: &SyntaxNode) -> Option<Request> {
    let Value::String(path) = &node.args.first()?.value else {
        return None;
    };
    if path.is_empty() || is_discovery_path(path) {
        return None;
    }
    let repeated = match node.props.iter().find(|(key, _)| key == "repeated") {
        None => Repeated::Reject,
        Some((_, scalar)) => match &scalar.value {
            Value::String(text) => Repeated::from_name(text)?,
            _ => return None,
        },
    };
    let project = node
        .children
        .iter()
        .filter(|child| child.name == "project")
        .flat_map(|child| child.args.iter())
        .filter_map(|scalar| match &scalar.value {
            Value::String(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    Some(Request {
        path: path.clone(),
        repeated,
        project,
        span: node.span,
    })
}
