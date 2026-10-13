//! Logical types contributed by `bindings.kdl`: roles, node
//! declarations, field bindings and their sources, collision and
//! duplicate-key policies, and child cardinality.

use kdlith_engine::{OrderedMap, Shaped, TypeShape, enum_type, object_type, union_type};

use super::{
    ContextPolicy, EffectGroup, ExactlyOneGroup, FieldName, NodeName, SlotName, TypeExpression,
    TypeName,
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
        "fields" => fields: OrderedMap<FieldName, FieldBinding>,
        "children" => children: OrderedMap<NodeName, ChildRule>,
        "effects" => effects: Vec<EffectGroup>,
        "groups" => groups: Vec<ExactlyOneGroup>,
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
    FieldBinding::SHAPE,
    Source::SHAPE,
    ArgumentSource::SHAPE,
    ArgsSource::SHAPE,
    PropertySource::SHAPE,
    ChildSource::SHAPE,
    ChildrenSource::SHAPE,
    EntriesSource::SHAPE,
    ContextSource::SHAPE,
    Collision::SHAPE,
    DuplicateKeys::SHAPE,
    ChildRule::SHAPE,
];
