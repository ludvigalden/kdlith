use kdlith_diagnostic::DiagnosticRecord;
use serde::Serialize;

use crate::identity::{Identity, NodeId, Reference};
use crate::origin::OriginId;
use crate::source::{Location, SourceId};
use crate::value::{Field, Value};

/// How far an item has been checked. Provisional items passed local
/// checks but wait on references, deferred bindings, or expansions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ValidationState {
    Valid,
    Provisional,
    Invalid,
}

/// A validated node of the record role, with its typed fields.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct BoundRecord {
    pub node: NodeId,
    pub type_name: String,
    pub fields: Vec<Field>,
    pub origin: OriginId,
    pub validation: ValidationState,
}

/// A node of the declaration role: registers an identity and exposes
/// typed metadata for later lookup and linking.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct BoundDeclaration {
    pub node: NodeId,
    pub identity: Identity,
    pub type_name: String,
    pub fields: Vec<Field>,
    pub origin: OriginId,
    pub validation: ValidationState,
}

/// An assignment or overlay preserved unresolved, so a resolver can apply
/// layer precedence and merge policy later.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct BoundContribution {
    pub node: NodeId,
    pub target: Reference,
    pub value: Value,
    pub layer: Option<String>,
    pub scope: Option<String>,
    pub origin: OriginId,
    pub validation: ValidationState,
}

/// A request to invoke another source: the original expression, any
/// alias it binds, and the import site. Scheduling is not done here.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ImportRequest {
    pub node: NodeId,
    pub expression: String,
    pub alias: Option<String>,
    pub site: Location,
    pub origin: OriginId,
}

/// A value whose expected type comes from a referenced declaration, kept
/// raw until that declaration is known.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct PendingBinding {
    pub node: NodeId,
    pub field: String,
    pub against: Reference,
    pub raw: Value,
    pub origin: OriginId,
}

/// An unresolved reference to a reusable declaration group, emitted at
/// its expansion point so expanded members keep their position.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ExpansionRequest {
    pub node: NodeId,
    pub target: Reference,
    pub member: Option<String>,
    pub site: Location,
    pub origin: OriginId,
}

/// The end of a scope: a container node, or the whole source when
/// `scope` is absent. Completion checks for the scope may now run.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ScopeCompletion {
    pub source: SourceId,
    pub scope: Option<NodeId>,
}

/// One item of a processing session over the full role vocabulary.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "item", content = "data", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Item {
    Record(BoundRecord),
    Declaration(BoundDeclaration),
    Contribution(BoundContribution),
    Import(ImportRequest),
    PendingBinding(PendingBinding),
    Expansion(ExpansionRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
}

/// One item of a schema-bound session without symbol roles: records,
/// imports, diagnostics, and scope ends. Every bound item is an item.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "item", content = "data", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum BoundItem {
    Record(BoundRecord),
    Import(ImportRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
}

impl From<BoundItem> for Item {
    fn from(item: BoundItem) -> Self {
        match item {
            BoundItem::Record(record) => Item::Record(record),
            BoundItem::Import(request) => Item::Import(request),
            BoundItem::Diagnostic(record) => Item::Diagnostic(record),
            BoundItem::ScopeComplete(completion) => Item::ScopeComplete(completion),
        }
    }
}
