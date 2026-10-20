//! Logical types contributed by `bindings.kdl`: roles, node
//! declarations, field bindings and their sources, collision and
//! duplicate-key policies, and child cardinality.

use kdlith_engine::{OrderedMap, Shaped, TypeShape, ValueExpr, enum_type, object_type, union_type};

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
        "role" => role: Option<Role>,
        "type" => ty: Option<TypeName>,
        "parent" => parent: Option<NodeName>,
        "parents" => parents: Option<Vec<NodeName>>,
        "profile" => profile: Option<ProfileName>,
        "fields" => fields: OrderedMap<FieldName, FieldBinding>,
        "children" => children: OrderedMap<NodeName, ChildRule>,
        "effects" => effects: Vec<EffectGroup>,
        "groups" => groups: Vec<ExactlyOneGroup>,
        "descendants" => descendants: Vec<DescendantRule>,
        "sequence" => sequence: Option<SequenceRule>,
        "value" => value: Option<DependentBinding>,
        "cases" => cases: Option<CaseDispatch>,
        "identity" => identity: Option<IdentityBlock>,
    }
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
    /// A case dispatch: the logical type a node binds as, selected by
    /// property value, top-down with first match winning.
    pub struct CaseDispatch {
        "cases" => cases: Vec<CaseRule>,
        "otherwise" => otherwise: Option<Otherwise>,
    }
}

object_type! {
    /// One case: when the named property is present and scalar-equal to
    /// equals, the node binds as the type.
    pub struct CaseRule {
        "property" => property: FieldName,
        "equals" => equals: ValueExpr,
        "type" => ty: TypeExpression,
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
    }
}

object_type! {
    pub struct ChildrenSource {
        "name" => name: NodeName,
        "sources" => sources: Vec<Source>,
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
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    Role::SHAPE,
    NodeDef::SHAPE,
    DependentBinding::SHAPE,
    DependentTarget::SHAPE,
    BindingCodec::SHAPE,
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
    EntriesSource::SHAPE,
    ExpandSource::SHAPE,
    ContextSource::SHAPE,
    Collision::SHAPE,
    DuplicateKeys::SHAPE,
    ChildRule::SHAPE,
];
