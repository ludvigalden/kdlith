use kdlith_diagnostic::DiagnosticRecord;
use serde::{Deserialize, Serialize};

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
/// alias it binds, the caller's slots it projects, its repeat policy, and
/// the import site. Scheduling is not done here.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ImportRequest {
    pub node: NodeId,
    pub expression: String,
    pub alias: Option<String>,
    /// The caller's context slots the imported source receives, by name.
    pub project: Vec<String>,
    pub repeated: Repeated,
    pub site: Location,
    pub origin: OriginId,
}

/// How a source imported again in one session is handled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Repeated {
    /// A second import of the same source is an error.
    #[default]
    Reject,
    /// A second import under an equal projected context reuses the first
    /// invocation.
    Coalesce,
    /// Each import is a separate invocation.
    Instantiate,
}

impl Repeated {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "reject" => Repeated::Reject,
            "coalesce" => Repeated::Coalesce,
            "instantiate" => Repeated::Instantiate,
            _ => return None,
        })
    }
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

impl BoundRecord {
    pub fn new(
        node: NodeId,
        type_name: impl Into<String>,
        fields: Vec<Field>,
        origin: OriginId,
        validation: ValidationState,
    ) -> Self {
        Self {
            node,
            type_name: type_name.into(),
            fields,
            origin,
            validation,
        }
    }
}

impl BoundDeclaration {
    pub fn new(
        node: NodeId,
        identity: Identity,
        type_name: impl Into<String>,
        fields: Vec<Field>,
        origin: OriginId,
        validation: ValidationState,
    ) -> Self {
        Self {
            node,
            identity,
            type_name: type_name.into(),
            fields,
            origin,
            validation,
        }
    }
}

impl BoundContribution {
    /// A contribution with no layer or application scope yet.
    pub fn new(
        node: NodeId,
        target: Reference,
        value: Value,
        origin: OriginId,
        validation: ValidationState,
    ) -> Self {
        Self {
            node,
            target,
            value,
            layer: None,
            scope: None,
            origin,
            validation,
        }
    }
}

impl ImportRequest {
    /// A request without an alias or projection that rejects repeats.
    pub fn new(
        node: NodeId,
        expression: impl Into<String>,
        site: Location,
        origin: OriginId,
    ) -> Self {
        Self {
            node,
            expression: expression.into(),
            alias: None,
            project: Vec::new(),
            repeated: Repeated::default(),
            site,
            origin,
        }
    }

    /// The request with the projection and repeat policy it was written with.
    #[must_use]
    pub fn with_policy(mut self, project: Vec<String>, repeated: Repeated) -> Self {
        self.project = project;
        self.repeated = repeated;
        self
    }
}

impl ScopeCompletion {
    pub fn new(source: SourceId, scope: Option<NodeId>) -> Self {
        Self { source, scope }
    }
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

/// One item of a schema-bound session: the role vocabulary a binder
/// yields without deferred mechanisms. Every bound item is an item.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "item", content = "data", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum BoundItem {
    Record(BoundRecord),
    Declaration(BoundDeclaration),
    Contribution(Box<BoundContribution>),
    Import(ImportRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
}

impl From<BoundItem> for Item {
    fn from(item: BoundItem) -> Self {
        match item {
            BoundItem::Record(record) => Item::Record(record),
            BoundItem::Declaration(declaration) => Item::Declaration(declaration),
            BoundItem::Contribution(contribution) => Item::Contribution(*contribution),
            BoundItem::Import(request) => Item::Import(request),
            BoundItem::Diagnostic(record) => Item::Diagnostic(record),
            BoundItem::ScopeComplete(completion) => Item::ScopeComplete(completion),
        }
    }
}

/// An item outside the bound vocabulary (pending bindings, expansions)
/// is handed back unchanged.
impl TryFrom<Item> for BoundItem {
    type Error = Item;

    fn try_from(item: Item) -> Result<Self, Item> {
        Ok(match item {
            Item::Record(record) => BoundItem::Record(record),
            Item::Declaration(declaration) => BoundItem::Declaration(declaration),
            Item::Contribution(contribution) => BoundItem::Contribution(Box::new(contribution)),
            Item::Import(request) => BoundItem::Import(request),
            Item::Diagnostic(record) => BoundItem::Diagnostic(record),
            Item::ScopeComplete(completion) => BoundItem::ScopeComplete(completion),
            other => return Err(other),
        })
    }
}
