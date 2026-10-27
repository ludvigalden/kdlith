//! Logical types contributed by `bindings.kdl`: roles, node
//! declarations, field bindings and their sources, collision and
//! duplicate-key policies, and child cardinality.

use kdlith_engine::{
    OrderedMap, Shaped, TypeShape, ValueExpr, enum_type, object_type, string_type, union_type,
};

use super::{
    ContextPolicy, DescendantRule, EffectGroup, ExactlyOneGroup, FieldName, IdentityBlock,
    MemberName, NodeName, ProfileName, SequenceRule, SlotName, TypeExpression, TypeName,
};

enum_type! {
    /// What the processing session does with a node's bound result.
    pub enum Role {
        "record" => Record,
        "declaration" => Declaration,
        "contribution" => Contribution,
        "context-control" => ContextControl,
        "source-import" => SourceImport,
        "container" => Container,
    }
}

object_type! {
    pub struct NodeDef {
        "name" => name: NodeName,
        "aliases" => aliases: Option<Vec<NodeName>>,
        "role" => role: Option<Role>,
        "type" => ty: Option<TypeName>,
        "parent" => parent: Option<NodeName>,
        "parents" => parents: Option<Vec<NodeName>>,
        "parent-group" => parent_group: Option<NodeGroupName>,
        "root" => root: Option<bool>,
        "profile" => profile: Option<ProfileName>,
        "fields" => fields: OrderedMap<FieldName, FieldBinding>,
        "children" => children: OrderedMap<NodeName, ChildRule>,
        "children-groups" => children_groups: OrderedMap<NodeGroupName, ChildRule>,
        "effects" => effects: Vec<EffectGroup>,
        "groups" => groups: Vec<ExactlyOneGroup>,
        "descendants" => descendants: Vec<DescendantRule>,
        "sequence" => sequence: Option<SequenceRule>,
        "value" => value: Option<DependentBinding>,
        "value-from-path" => value_from_path: Option<PathValueBinding>,
        "cases" => cases: Option<CaseDispatch>,
        "identity" => identity: Option<IdentityBlock>,
        "description" => description: Option<String>,
        "annotations" => annotations: Option<Vec<NodeAnnotation>>,
    }
}

impl NodeDef {
    /// The identity domain its declarations register in.
    pub const DOMAIN: &str = "NodeDef";
}

string_type! {
    pub struct AnnotationName;
}

enum_type! {
    pub enum AnnotationRequirement {
        "required" => Required,
    }
}

object_type! {
    pub struct NodeAnnotation {
        "name" => name: AnnotationName,
        "value" => value: ValueExpr,
    }
}

object_type! {
    pub struct AnnotationDef {
        "name" => name: AnnotationName,
        "type" => ty: TypeExpression,
        "required" => required: Option<AnnotationRequirement>,
    }
}

impl AnnotationDef {
    /// The identity domain its declarations register in.
    pub const DOMAIN: &str = "AnnotationDef";
}

object_type! {
    /// A dependent payload binding: how a node's remaining payload binds
    /// against the declaration a field of the same node references.
    pub struct DependentBinding {
        "against" => against: DependentTarget,
        "codec" => codec: BindingCodec,
    }
}

object_type! {
    /// The field of the same node holding the reference, and the field of
    /// the referenced declaration's type holding the expected payload type.
    pub struct DependentTarget {
        "target" => target: FieldName,
        "member" => member: FieldName,
    }
}

enum_type! {
    /// The registered binding codecs a dependent value may name; an unknown
    /// codec fails closed, and a known one the reading processor does not
    /// install compiles and binds as a pending typed-binding item.
    pub enum BindingCodec {
        "argument-tail" => ArgumentTail,
        "fixture-value" => FixtureValue,
        "from-kind" => FromKind,
    }
}

object_type! {
    /// A path-typed payload binding: how a node's remaining payload binds
    /// in the type of the field its path field names, resolved against
    /// the subject type the node's case dispatch selects.
    pub struct PathValueBinding {
        "field" => field: FieldName,
    }
}

object_type! {
    /// A case dispatch: the logical type a node binds as, selected by
    /// property value, top-down with first match winning.
    pub struct CaseDispatch {
        "cases" => cases: Vec<CaseRule>,
        "otherwise" => otherwise: Option<Otherwise>,
    }
}

object_type! {
    /// One case: when the named property is present and scalar-equal
    /// to equals, the node binds as type, and the case's own field
    /// and children bindings add to the node's common ones for that
    /// instance. A case-level binding naming what a common binding of
    /// the same node names is rejected when the definition compiles.
    pub struct CaseRule {
        "property" => property: FieldName,
        "equals" => equals: ValueExpr,
        "type" => ty: TypeExpression,
        "fields" => fields: OrderedMap<FieldName, FieldBinding>,
        "children" => children: OrderedMap<NodeName, ChildRule>,
    }
}

object_type! {
    /// The fallback of a case dispatch: the type a node binds as when no
    /// case matches.
    pub struct Otherwise {
        "type" => ty: TypeExpression,
    }
}

object_type! {
    pub struct FieldBinding {
        "type" => ty: Option<TypeExpression>,
        "arg" => arg: Option<u64>,
        "property" => property: Option<FieldName>,
        "child" => child: Option<NodeName>,
        "context" => context: Option<SlotName>,
        // The context slot this field's unqualified references take their
        // qualifier from, overriding the field's reference type's choice.
        "qualify-from" => qualify_from: Option<SlotName>,
        "sources" => sources: Vec<Source>,
        "collision" => collision: Option<Collision>,
        "duplicate-keys" => duplicate_keys: Option<DuplicateKeys>,
    }
}

union_type! {
    /// Where a field's value comes from.
    pub enum Source {
        "argument" => Argument(ArgumentSource),
        "args" => Args(ArgsSource),
        "property" => Property(PropertySource),
        "child" => Child(ChildSource),
        "children" => Children(ChildrenSource),
        "children-group" => ChildrenGroup(ChildrenGroupSource),
        "child-group" => ChildGroup(ChildGroupSource),
        "entries" => Entries(EntriesSource),
        "expand" => Expand(ExpandSource),
        "context" => Context(ContextSource),
    }
}

object_type! {
    pub struct ArgumentSource {
        "index" => index: u64,
    }
}

object_type! {
    pub struct ArgsSource {}
}

object_type! {
    pub struct PropertySource {
        "name" => name: FieldName,
    }
}

object_type! {
    pub struct ChildSource {
        "name" => name: NodeName,
        "sources" => sources: Vec<Source>,
        "compact" => compact: Option<bool>,
    }
}

object_type! {
    pub struct ChildrenSource {
        "name" => name: NodeName,
        "sources" => sources: Vec<Source>,
        "compact" => compact: Option<bool>,
    }
}

object_type! {
    /// A pending children-group reference in a from block: one children
    /// source per member of the named group splices in at this source's
    /// position when the schema is compiled, as the hand-written form
    /// spells them.
    pub struct ChildrenGroupSource {
        "name" => name: NodeGroupName,
    }
}

object_type! {
    /// A pending child-group reference in a from block: one child
    /// source per member of the named group splices in at this source's
    /// position when the schema is compiled, as the hand-written form
    /// spells them.
    pub struct ChildGroupSource {
        "name" => name: NodeGroupName,
    }
}

object_type! {
    pub struct EntriesSource {
        "node" => node: NodeName,
        "key" => key: String,
        "value" => value: TypeName,
    }
}

object_type! {
    /// Declaration-group expansion: the entries of the named keyed member
    /// of the named declaration splice into the collection being bound.
    pub struct ExpandSource {
        "target" => target: TypeName,
        "member" => member: MemberName,
    }
}

object_type! {
    pub struct ContextSource {
        "slot" => slot: SlotName,
        "policy" => policy: Option<ContextPolicy>,
    }
}

enum_type! {
    pub enum Collision {
        "reject" => Reject,
    }
}

enum_type! {
    pub enum DuplicateKeys {
        "reject" => Reject,
    }
}

object_type! {
    pub struct ChildRule {
        "min-occurs" => min_occurs: Option<u64>,
        "max-occurs" => max_occurs: Option<u64>,
        "branches" => branches: Option<Vec<Condition>>,
    }
}

union_type! {
    pub enum Condition {
        "when" => When(PropertyCondition),
        "when-all" => WhenAll(ConditionGroup),
        "when-any" => WhenAny(ConditionGroup),
    }
}

object_type! {
    pub struct PropertyCondition {
        "property" => property: FieldName,
        "equals" => equals: ValueExpr,
        "min-occurs" => min_occurs: Option<u64>,
        "max-occurs" => max_occurs: Option<u64>,
    }
}

object_type! {
    pub struct ConditionGroup {
        "members" => members: Vec<Condition>,
        "min-occurs" => min_occurs: Option<u64>,
        "max-occurs" => max_occurs: Option<u64>,
    }
}

string_type! {
    pub struct NodeGroupName;
}

object_type! {
    pub struct NodeGroupDef {
        "name" => name: NodeGroupName,
        "nodes" => nodes: Vec<NodeName>,
    }
}

impl NodeGroupDef {
    /// The identity domain its declarations register in.
    pub const DOMAIN: &str = "NodeGroupDef";
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    Role::SHAPE,
    NodeDef::SHAPE,
    AnnotationName::SHAPE,
    AnnotationRequirement::SHAPE,
    NodeAnnotation::SHAPE,
    AnnotationDef::SHAPE,
    DependentBinding::SHAPE,
    DependentTarget::SHAPE,
    BindingCodec::SHAPE,
    PathValueBinding::SHAPE,
    CaseDispatch::SHAPE,
    CaseRule::SHAPE,
    Otherwise::SHAPE,
    FieldBinding::SHAPE,
    Source::SHAPE,
    ArgumentSource::SHAPE,
    ArgsSource::SHAPE,
    PropertySource::SHAPE,
    ChildSource::SHAPE,
    ChildrenSource::SHAPE,
    ChildrenGroupSource::SHAPE,
    ChildGroupSource::SHAPE,
    EntriesSource::SHAPE,
    ExpandSource::SHAPE,
    ContextSource::SHAPE,
    Collision::SHAPE,
    DuplicateKeys::SHAPE,
    ChildRule::SHAPE,
    Condition::SHAPE,
    PropertyCondition::SHAPE,
    ConditionGroup::SHAPE,
    NodeGroupName::SHAPE,
    NodeGroupDef::SHAPE,
];
