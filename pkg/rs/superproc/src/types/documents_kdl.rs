//! Logical types contributed by `documents.kdl`: definition roots,
//! structural document rules, source imports, and deferrals.

use kdlith_engine::{Shaped, TypeShape, enum_type, object_type, string_type};

use super::{NodeName, SlotName};

string_type! {
    pub struct DialectName;
}

string_type! {
    pub struct SchemaName;
}

string_type! {
    pub struct MechanismName;
}

object_type! {
    pub struct DialectDef {
        "name" => name: DialectName,
        "version" => version: String,
    }
}

object_type! {
    pub struct SchemaDef {
        "name" => name: SchemaName,
        "version" => version: Option<String>,
    }
}

object_type! {
    pub struct DocumentRule {
        "first" => first: Option<FirstRule>,
        "following" => following: Vec<NodeName>,
    }
}

object_type! {
    pub struct FirstRule {
        "exactly-one" => exactly_one: Vec<NodeName>,
    }
}

object_type! {
    pub struct ExactlyOneGroup {
        "nodes" => nodes: Vec<NodeName>,
    }
}

enum_type! {
    pub enum RepeatedImport {
        "reject" => Reject,
        "coalesce" => Coalesce,
        "instantiate" => Instantiate,
    }
}

object_type! {
    pub struct ImportRequest {
        "path" => path: String,
        "project" => project: Option<Vec<SlotName>>,
        "repeated" => repeated: RepeatedImport,
    }
}

object_type! {
    pub struct Deferral {
        "mechanism" => mechanism: MechanismName,
        "note" => note: String,
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    DialectName::SHAPE,
    SchemaName::SHAPE,
    MechanismName::SHAPE,
    DialectDef::SHAPE,
    SchemaDef::SHAPE,
    DocumentRule::SHAPE,
    FirstRule::SHAPE,
    ExactlyOneGroup::SHAPE,
    RepeatedImport::SHAPE,
    ImportRequest::SHAPE,
    Deferral::SHAPE,
];
