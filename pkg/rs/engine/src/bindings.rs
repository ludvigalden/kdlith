//! The BindingCatalog: node declarations with their roles, field
//! bindings, child rules, and the compiled dispatch and scope tables.

use std::collections::BTreeMap;

use kdlith_model::Value;

use crate::diag::Site;
use crate::names::Sym;
use crate::structure::ScopeRules;
use crate::types::TypeExpr;

/// A node declaration in its catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeDeclId(u32);

impl NodeDeclId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn from_index(at: usize) -> Self {
        Self(u32::try_from(at).unwrap_or(u32::MAX))
    }
}

/// What a session does with a node's bound result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Role {
    Record,
    Declaration,
    Contribution,
    ContextControl,
    SourceImport,
    Container,
}

impl Role {
    /// The role as a definition spells it.
    pub fn name(self) -> &'static str {
        match self {
            Role::Record => "record",
            Role::Declaration => "declaration",
            Role::Contribution => "contribution",
            Role::ContextControl => "context-control",
            Role::SourceImport => "source-import",
            Role::Container => "container",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "record" => Role::Record,
            "declaration" => Role::Declaration,
            "contribution" => Role::Contribution,
            "context-control" => Role::ContextControl,
            "source-import" => Role::SourceImport,
            "container" => Role::Container,
            _ => return None,
        })
    }
}

/// Where a node may appear.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Placement {
    /// A root node of a document.
    Root,
    /// Under any node with one of these names.
    Under(Vec<Sym>),
    /// A root node of a document, and under any node with one of these
    /// names (`root=#true` beside `parent=` or `parents`).
    RootAndUnder(Vec<Sym>),
}

impl Placement {
    /// The parents the node may appear under; empty for a root-only node.
    pub fn parents(&self) -> &[Sym] {
        match self {
            Placement::Root => &[],
            Placement::Under(parents) | Placement::RootAndUnder(parents) => parents,
        }
    }

    /// Whether the node may appear at the root of a document.
    pub fn at_root(&self) -> bool {
        matches!(self, Placement::Root | Placement::RootAndUnder(_))
    }
}

/// The parent half of a dispatch key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ParentKey {
    Root,
    Under(Sym),
}

/// One node declaration, compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NodeDecl {
    pub name: Sym,
    /// `None`: bound as part of its parent, not a processing item.
    pub role: Option<Role>,
    /// The logical object type whose fields the bindings populate.
    pub ty: Option<Sym>,
    pub placement: Placement,
    /// The node group the declaration's `parents` names with `group=`;
    /// compiled into `placement`, one parent per member.
    pub parent_group: Option<Sym>,
    /// The profile this declaration belongs to, when it carries
    /// `profile=NAME`: the declaration is admitted only under that
    /// profile.
    pub profile: Option<Sym>,
    pub fields: Vec<FieldBinding>,
    pub children: Vec<ChildRule>,
    /// The `children-group` rules, each named by its node group;
    /// compiled into `children`, one rule per member.
    pub children_groups: Vec<ChildRule>,
    pub effects: Vec<EffectGroup>,
    pub groups: Vec<Vec<Sym>>,
    /// Occurrence bounds over this node's whole subtree, checked when
    /// this scope completes; never across an import boundary.
    pub descendants: Vec<DescendantRule>,
    /// The ordered sequence the node's immediate children must match.
    pub sequence: Option<SequenceDecl>,
    /// The node's dependent payload binding, if it declares one.
    pub value: Option<DependentBinding>,
    /// The case dispatch that selects the type a node binds as, top-down,
    /// first match winning.
    pub cases: Vec<CaseRule>,
    /// The fallback type when no case matches.
    pub otherwise: Option<TypeExpr>,
    /// The identity block the declaration composes its qualified symbol
    /// from, when it declares one.
    pub identity: Option<IdentityDecl>,
    /// The declaration's documentation, as `description=` spells it.
    /// Metadata only: nothing in binding, linking, or provenance reads
    /// it.
    pub description: Option<String>,
    /// The annotation facts the declaration carries. Metadata only:
    /// checked when the definition compiles; nothing in binding,
    /// linking, or provenance reads them.
    pub annotations: Vec<NodeAnnotation>,
    pub site: Site,
}

/// One annotation fact on a node declaration: the name of a
/// schema-declared annotation and its value as written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NodeAnnotation {
    pub name: Sym,
    pub value: Value,
}

/// An occurrence bound over the declaring node's subtree.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DescendantRule {
    pub name: Sym,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub site: Site,
}

/// An ordered sequence of steps a node's immediate children must match.
/// Repetition stays with each step's bounds; there are no
/// unbounded-repetition groups.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SequenceDecl {
    pub steps: Vec<SequenceStep>,
    pub site: Site,
}

/// One step of an ordered sequence: one node name or a group of
/// alternatives. Absent bounds mean exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SequenceStep {
    /// The node names the step admits.
    pub alternatives: Vec<Sym>,
    pub min: u64,
    pub max: u64,
    pub site: Site,
}

/// The identity a declaration registers: its domain and the fields its
/// qualified symbol composes from, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdentityDecl {
    pub domain: Sym,
    pub fields: Vec<Sym>,
    pub site: Site,
}

/// How a node's remaining payload binds against the declaration a field
/// of the same node references.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DependentBinding {
    /// The field of the same node holding the reference.
    pub target: Sym,
    /// The field of the referenced declaration's type holding the
    /// expected payload type.
    pub member: Sym,
    /// The registered binding codec that binds the payload.
    pub codec: BindingCodec,
    pub site: Site,
}

/// The registered binding codecs a dependent value may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BindingCodec {
    /// The remaining arguments, typed per the expected type, with no
    /// scalar-to-list promotion.
    ArgumentTail,
    /// The type-directed recursive form.
    FixtureValue,
    /// The payload shape selected by the value the resolved declaration
    /// holds in its member field.
    FromKind,
}

impl BindingCodec {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "argument-tail" => BindingCodec::ArgumentTail,
            "fixture-value" => BindingCodec::FixtureValue,
            "from-kind" => BindingCodec::FromKind,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            BindingCodec::ArgumentTail => "argument-tail",
            BindingCodec::FixtureValue => "fixture-value",
            BindingCodec::FromKind => "from-kind",
        }
    }
}

/// One case of a case dispatch: when the named property is present and
/// scalar-equal to `equals`, the node binds as `ty`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CaseRule {
    pub property: Sym,
    pub equals: Value,
    pub ty: TypeExpr,
    pub site: Site,
}

/// How a scope owner's positional arguments are consumed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Arity {
    /// Arguments `0..indexed` are each bound exactly once, counting the
    /// argument an entries envelope consumes as the key.
    pub indexed: u64,
    /// Arguments from `indexed` on are collected by an `args` source.
    pub rest: bool,
}

/// One logical field and the sources that populate it. Shorthands and
/// `from` blocks compile to the same sources.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldBinding {
    pub name: Sym,
    /// An inline type expression; absent when the node's type gives it.
    pub ty: Option<TypeExpr>,
    pub sources: Vec<Source>,
    /// The context slot this field's unqualified references take their
    /// qualifier from, overriding the field's reference type's choice.
    /// `None` defers to the reference type, else the `namespace` slot.
    pub qualify_from: Option<Sym>,
    /// Pending `children-group` from-sources, spliced one children
    /// source per member into `sources` at `position` by the group
    /// expansion pass — the hand-written repeated-source form.
    pub(crate) pending_groups: Vec<PendingGroupSource>,
    /// Required when two or more alternative sources (not `children` or
    /// `context`) populate the field; with `reject`, supplying more than
    /// one is an error even when the values are equal.
    pub collision: Option<Collision>,
    pub duplicate_keys: Option<DuplicateKeys>,
    pub site: Site,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source {
    Argument(u64),
    /// The remaining positional arguments, in order.
    Args,
    Property(Sym),
    /// One child by name; nested sources select from its contents.
    /// `compact` admits the child spelled as a property of the node the
    /// source selects from, carrying the child's one argument.
    Child {
        name: Sym,
        sources: Vec<Source>,
        compact: bool,
    },
    /// Repeated children by name, collected in order.
    Children {
        name: Sym,
        sources: Vec<Source>,
        compact: bool,
    },
    /// A keyed envelope: argument `key` of each `node` child is the map
    /// key; the rest binds as `value`.
    Entries {
        node: Sym,
        key: u64,
        value: Sym,
    },
    /// Declaration-group expansion: the entries of the named keyed
    /// member of the named declaration splice into the collection at the
    /// expansion point, in declaration order.
    Expand {
        target: Sym,
        member: Sym,
    },
    Context {
        slot: Sym,
        policy: Option<ContextPolicy>,
    },
}

/// A pending `children-group` from-source, resolved by the group
/// expansion pass after the declaration pass completes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingGroupSource {
    /// Where in the field's `sources` the spliced members land.
    pub(crate) position: usize,
    pub(crate) name: Sym,
    pub(crate) site: Site,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Collision {
    Reject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DuplicateKeys {
    Reject,
}

/// How a context-bound field relates to an explicit value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextPolicy {
    /// The slot supplies the field only when no explicit value is present.
    Fallback,
    /// An explicit value must agree with the slot.
    Fixed,
}

/// A `children NAME` rule: permission plus occurrence bounds. Its base
/// bounds apply to the instances no when branch matches.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildRule {
    pub name: Sym,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub branches: Vec<WhenBranch>,
    pub site: Site,
}

/// One when branch of a children rule: for the instances its condition
/// matches, its bounds replace the rule's base bounds, both at once.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WhenBranch {
    pub condition: Condition,
    pub min: Option<u64>,
    pub max: Option<u64>,
}

/// A condition over a node's own bound property values.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Condition {
    /// The property holds a value scalar-equal to `equals`; an absent
    /// property is an unmet condition.
    Equals { property: Sym, equals: Value },
    /// Every member holds.
    All(Vec<Condition>),
    /// At least one member holds.
    Any(Vec<Condition>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EffectTarget {
    Following,
    Children,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EffectGroup {
    pub target: EffectTarget,
    pub effects: Vec<Effect>,
    pub site: Site,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Effect {
    Set {
        slot: Sym,
        from: SetFrom,
        /// The layer a set on a layered slot names; `None` on every
        /// other slot.
        layer: Option<Sym>,
    },
    /// The slot becomes unset, not null.
    Remove {
        slot: Sym,
    },
    Clear {
        namespace: Sym,
    },
    /// Domain computation the installing processor implements.
    Handler {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SetFrom {
    Field(Sym),
    Value(Value),
}

/// A compiled scope: the children of one declared node, of a child a
/// field consumes with nested sources, or of a document's root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u32);

impl ScopeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn from_index(at: usize) -> Self {
        Self(u32::try_from(at).unwrap_or(u32::MAX))
    }
}

/// How a child arriving in a scope binds, decided at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChildPlan {
    /// Through the node declared for this parent and name; `key` is the
    /// argument an entries envelope consumes as the map key.
    Declared { decl: NodeDeclId, key: Option<u64> },
    /// Through the nested sources of the fields that consume it.
    Nested(ScopeId),
    /// A consumed child with no declaration: exactly one positional value.
    Scalar,
}

/// Everything a binder needs about one scope, compiled once.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Scope {
    /// The name of the node owning the scope; `None` for the root scope.
    pub name: Option<Sym>,
    pub decl: Option<NodeDeclId>,
    pub rules: ScopeRules,
    /// The owning node's positional arguments the bindings consume.
    pub arity: Arity,
    /// The owning node's properties the bindings consume.
    pub properties: Vec<Sym>,
    pub(crate) plans: BTreeMap<Sym, ChildPlan>,
    /// The children that may also be spelled as a property carrying
    /// their one argument.
    pub(crate) compact: Vec<Sym>,
}

impl Scope {
    /// The plan for a child; `None` when no binding admits it here.
    pub fn child(&self, name: Sym) -> Option<ChildPlan> {
        self.plans.get(&name).copied()
    }

    /// Whether a property named `name` spells a child of that name.
    pub fn admits_compact(&self, name: Sym) -> bool {
        self.compact.contains(&name)
    }
}

/// Every node declaration of one definition with its dispatch table and
/// compiled scopes; the root scope dispatches a document's root nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingCatalog {
    pub(crate) nodes: Vec<NodeDecl>,
    pub(crate) dispatch: BTreeMap<(ParentKey, Sym), NodeDeclId>,
    pub(crate) scopes: Vec<Scope>,
    pub(crate) decl_scopes: Vec<ScopeId>,
    pub(crate) root: ScopeId,
}

impl BindingCatalog {
    pub fn get(&self, id: NodeDeclId) -> Option<&NodeDecl> {
        self.nodes.get(id.index())
    }

    /// Every declaration in contribution order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeDeclId, &NodeDecl)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(at, decl)| (NodeDeclId::from_index(at), decl))
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The declaration for a name under a parent: deterministic, never
    /// trying declarations in turn.
    pub fn dispatch(&self, parent: ParentKey, name: Sym) -> Option<NodeDeclId> {
        self.dispatch.get(&(parent, name)).copied()
    }

    pub fn root_scope(&self) -> ScopeId {
        self.root
    }

    /// The scope of a declared node's children.
    pub fn decl_scope(&self, id: NodeDeclId) -> Option<ScopeId> {
        self.decl_scopes.get(id.index()).copied()
    }

    pub fn scope(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.index())
    }

    /// The plan for a child arriving in a scope.
    pub fn child(&self, scope: ScopeId, name: Sym) -> Option<ChildPlan> {
        self.scope(scope)?.child(name)
    }

    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }
}
