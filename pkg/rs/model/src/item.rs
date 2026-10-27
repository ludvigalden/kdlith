use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use serde::{Deserialize, Serialize};

use crate::identity::{Identity, NodeId, Reference};
use crate::origin::OriginId;
use crate::source::{Location, SourceId};
use crate::sources::SourceSet;
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
    /// The declared node that bound the record, by name: several nodes
    /// may bind one type. Not part of the item's serialized form.
    #[serde(skip)]
    pub node_name: String,
    pub fields: Vec<Field>,
    pub origin: OriginId,
    pub validation: ValidationState,
    /// The span of the node this record is; not part of the item's
    /// serialized form.
    #[serde(skip)]
    pub span: ByteSpan,
    /// Where the node was written, with line and column; not part of
    /// the item's serialized form.
    #[serde(skip)]
    pub site: Option<Location>,
    /// The nearest enclosing node whose role yields an item, in the same
    /// source; `None` at the document's root level. Not part of the
    /// item's serialized form.
    #[serde(skip)]
    pub parent: Option<NodeId>,
}

/// A node of the declaration role: registers an identity and exposes
/// typed metadata for later lookup and linking.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct BoundDeclaration {
    pub node: NodeId,
    pub identity: Identity,
    pub type_name: String,
    /// The declared node that bound the declaration, by name: several
    /// nodes may bind one type. Not part of the item's serialized form.
    #[serde(skip)]
    pub node_name: String,
    pub fields: Vec<Field>,
    pub origin: OriginId,
    pub validation: ValidationState,
    /// The span of the node this declaration is; not part of the item's
    /// serialized form.
    #[serde(skip)]
    pub span: ByteSpan,
    /// Where the node was written, with line and column; not part of
    /// the item's serialized form.
    #[serde(skip)]
    pub site: Option<Location>,
    /// The nearest enclosing node whose role yields an item, as on a
    /// record; not part of the item's serialized form.
    #[serde(skip)]
    pub parent: Option<NodeId>,
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
    /// The span of the node this contribution is; not part of the
    /// item's serialized form.
    #[serde(skip)]
    pub span: ByteSpan,
    /// Where the node was written, with line and column; not part of
    /// the item's serialized form.
    #[serde(skip)]
    pub site: Option<Location>,
    /// The members of the contributed value as fields, carrying each
    /// member's provenance; not part of the item's serialized form.
    #[serde(skip)]
    pub members: Vec<Field>,
}

/// One contribution target as the registered resolver settled it within
/// one application scope: the target, its resolved value, the layers
/// whose records reached the value, highest precedence first, and the
/// scope the records shared (none for scope-less records). Derived from
/// preserved records after a reading; never bound from a node of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct ResolvedContribution {
    pub target: Reference,
    pub value: Value,
    pub layers: Vec<String>,
    pub scope: Option<String>,
}

impl ResolvedContribution {
    pub fn new(
        target: Reference,
        value: Value,
        layers: Vec<String>,
        scope: Option<String>,
    ) -> Self {
        Self {
            target,
            value,
            layers,
            scope,
        }
    }
}

/// A request to invoke another source: the original expression, any
/// namespace it binds, the caller's slots it projects, its repeat policy, and
/// the import site. Scheduling is not done here.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ImportRequest {
    pub node: NodeId,
    pub expression: String,
    /// The paths the expression resolved to, as the reading shows them,
    /// in resolution order; empty when it did not resolve.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    pub namespace: Option<String>,
    /// The projection tokens the imported source receives, by name as
    /// written: each names a declared context slot, or a declared
    /// namespace standing for every slot in it.
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
    /// The node the payload belongs to, as written; not part of the
    /// item's serialized form.
    #[serde(skip)]
    pub node_name: String,
    /// The member of the referenced declaration's type that carries the
    /// expected payload type; not part of the item's serialized form.
    #[serde(skip)]
    pub member: String,
    /// The binding codec the payload awaits; not part of the item's
    /// serialized form.
    #[serde(skip)]
    pub codec: String,
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
    /// The declaration whose members the group splices into, when the
    /// expansion point is written among them.
    pub owner: Option<String>,
    /// How many like members of the owner precede the expansion point.
    pub at: usize,
}

/// A document whose items follow: the source identity its records,
/// declarations, and completions carry, and the path as the reading
/// shows it. Emitted ahead of a document's first item so a stream
/// consumer can attribute every later item to its document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceDocument {
    pub source: SourceId,
    pub path: String,
    /// The content digest of the revision the items were bound from,
    /// stamped with its algorithm; not part of the item's serialized
    /// form.
    #[serde(skip)]
    pub digest: Option<String>,
}

impl SourceDocument {
    pub fn new(source: SourceId, path: impl Into<String>) -> Self {
        Self {
            source,
            path: path.into(),
            digest: None,
        }
    }

    /// The document with the digest of the revision it was read at.
    #[must_use]
    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }
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
        node_name: impl Into<String>,
        type_name: impl Into<String>,
        fields: Vec<Field>,
        origin: OriginId,
        validation: ValidationState,
        span: ByteSpan,
    ) -> Self {
        Self {
            node,
            type_name: type_name.into(),
            node_name: node_name.into(),
            fields,
            origin,
            validation,
            span,
            site: None,
            parent: None,
        }
    }

    /// The record with the location its node was written at.
    #[must_use]
    pub fn with_site(mut self, site: Location) -> Self {
        self.site = Some(site);
        self
    }

    /// The record with the enclosing node its item belongs under.
    #[must_use]
    pub fn with_parent(mut self, parent: Option<NodeId>) -> Self {
        self.parent = parent;
        self
    }
}

impl BoundDeclaration {
    /// The declaration a bound node forms once its identity is composed:
    /// a declaration carries everything a record does, plus the identity
    /// it registers.
    pub fn from_record(record: BoundRecord, identity: Identity) -> Self {
        let BoundRecord {
            node,
            type_name,
            node_name,
            fields,
            origin,
            validation,
            span,
            site,
            parent,
        } = record;
        Self {
            node,
            identity,
            type_name,
            node_name,
            fields,
            origin,
            validation,
            span,
            site,
            parent,
        }
    }

    /// The declaration with the location its node was written at.
    #[must_use]
    pub fn with_site(mut self, site: Location) -> Self {
        self.site = Some(site);
        self
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
        span: ByteSpan,
    ) -> Self {
        Self {
            node,
            target,
            value,
            layer: None,
            scope: None,
            origin,
            validation,
            span,
            site: None,
            members: Vec::new(),
        }
    }

    /// The contribution with the location its node was written at and
    /// its value's members as fields carrying their provenance.
    #[must_use]
    pub fn with_provenance(mut self, site: Location, members: Vec<Field>) -> Self {
        self.site = Some(site);
        self.members = members;
        self
    }
}

impl ImportRequest {
    /// A request without a namespace or projection that rejects repeats.
    pub fn new(
        node: NodeId,
        expression: impl Into<String>,
        site: Location,
        origin: OriginId,
    ) -> Self {
        Self {
            node,
            expression: expression.into(),
            targets: Vec::new(),
            namespace: None,
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

impl PendingBinding {
    pub fn new(
        node: NodeId,
        field: impl Into<String>,
        against: Reference,
        raw: Value,
        origin: OriginId,
    ) -> Self {
        Self {
            node,
            field: field.into(),
            against,
            raw,
            origin,
            node_name: String::new(),
            member: String::new(),
            codec: String::new(),
        }
    }

    /// The binding this payload awaits: the node's name, the referenced
    /// type's member, and the codec.
    pub fn with_dependent(
        mut self,
        node_name: impl Into<String>,
        member: impl Into<String>,
        codec: impl Into<String>,
    ) -> Self {
        self.node_name = node_name.into();
        self.member = member.into();
        self.codec = codec.into();
        self
    }
}

impl ExpansionRequest {
    /// A request for the group `target` names, outside any owner.
    pub fn new(node: NodeId, target: Reference, site: Location, origin: OriginId) -> Self {
        Self {
            node,
            target,
            member: None,
            site,
            origin,
            owner: None,
            at: 0,
        }
    }

    /// The request written among `owner`'s members, after `at` of them.
    #[must_use]
    pub fn among(mut self, owner: impl Into<String>, at: usize) -> Self {
        self.owner = Some(owner.into());
        self.at = at;
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
    /// The items of this document follow, starting with the next item.
    Document(SourceDocument),
    Record(BoundRecord),
    Declaration(BoundDeclaration),
    Contribution(BoundContribution),
    Import(ImportRequest),
    PendingBinding(PendingBinding),
    Expansion(ExpansionRequest),
    Diagnostic(Box<DiagnosticRecord>),
    ScopeComplete(ScopeCompletion),
    /// A contribution target the registered resolver settled, after the
    /// reading's documents.
    Resolution(ResolvedContribution),
    /// The sources the reading used, once, after its documents.
    Sources(SourceSet),
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
    PendingBinding(PendingBinding),
    Expansion(ExpansionRequest),
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
            BoundItem::PendingBinding(pending) => Item::PendingBinding(pending),
            BoundItem::Expansion(request) => Item::Expansion(request),
            BoundItem::Diagnostic(record) => Item::Diagnostic(record),
            BoundItem::ScopeComplete(completion) => Item::ScopeComplete(completion),
        }
    }
}

/// An item outside the bound vocabulary (documents, resolutions) is
/// handed back unchanged.
impl TryFrom<Item> for BoundItem {
    type Error = Item;

    fn try_from(item: Item) -> Result<Self, Item> {
        Ok(match item {
            Item::Record(record) => BoundItem::Record(record),
            Item::Declaration(declaration) => BoundItem::Declaration(declaration),
            Item::Contribution(contribution) => BoundItem::Contribution(Box::new(contribution)),
            Item::Import(request) => BoundItem::Import(request),
            Item::PendingBinding(pending) => BoundItem::PendingBinding(pending),
            Item::Expansion(request) => BoundItem::Expansion(request),
            Item::Diagnostic(record) => BoundItem::Diagnostic(record),
            Item::ScopeComplete(completion) => BoundItem::ScopeComplete(completion),
            other => return Err(other),
        })
    }
}
