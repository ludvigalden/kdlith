//! Logical types contributed by `types.kdl`: names, type expressions,
//! built-in scalars and formats, and the type declarations themselves.

use kdlith_engine::{
    OrderedMap, Shaped, TypeShape, ValueExpr, enum_type, object_type, string_type, union_type,
};

string_type! {
    pub struct TypeName;
}

string_type! {
    pub struct NodeName;
}

string_type! {
    pub struct FieldName;
}

string_type! {
    pub struct MemberName;
}

string_type! {
    pub struct ValidatorName;
}

string_type! {
    /// A type-description grammar inside a quoted string.
    pub struct TypeExpression;
}

enum_type! {
    pub enum BuiltinType {
        "String" => String,
        "Bool" => Bool,
        "Integer" => Integer,
        "Decimal" => Decimal,
        "U64" => U64,
        "ValueExpr" => ValueExpr,
    }
}

enum_type! {
    pub enum TypeConstructor {
        "optional" => Optional,
        "nullable" => Nullable,
        "list" => List,
        "map" => Map,
        "ordered-map" => OrderedMap,
        "union" => Union,
        "ref" => Ref,
    }
}

enum_type! {
    pub enum StringFormat {
        "kebab-case" => KebabCase,
        "camel-case" => CamelCase,
        "lower-snake" => LowerSnake,
        "upper-snake" => UpperSnake,
        "uuid" => Uuid,
        "zero-padded" => ZeroPadded,
    }
}

enum_type! {
    /// The sensitivity a logical field may declare; metadata that every
    /// output surface enforces through an explicit policy.
    pub enum Sensitivity {
        "secret" => Secret,
        "reference" => Reference,
    }
}

union_type! {
    /// A type declaration, discriminated by the declaring node's name.
    pub enum TypeDef {
        "string" => String(StringType),
        "enum" => Enum(EnumType),
        "object" => Object(ObjectType),
        "union" => Union(UnionType),
        "map" => Map(MapType),
        "codec" => Codec(CodecType),
    }
}

object_type! {
    pub struct StringType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "format" => format: Option<StringFormat>,
        "width" => width: Option<u64>,
        "pattern" => pattern: Option<String>,
        "validator" => validator: Option<ValidatorName>,
        "validator-version" => validator_version: Option<String>,
        "min-length" => min_length: Option<u64>,
    }
}

object_type! {
    pub struct EnumType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "members" => members: OrderedMap<MemberName, EnumMember>,
    }
}

object_type! {
    pub struct EnumMember {
        "description" => description: Option<String>,
    }
}

object_type! {
    pub struct ObjectType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "fields" => fields: OrderedMap<FieldName, FieldDef>,
        "constraints" => constraints: Vec<Constraint>,
    }
}

object_type! {
    pub struct FieldDef {
        "type" => ty: TypeExpression,
        "description" => description: Option<String>,
        "documented-default" => documented_default: Option<ValueExpr>,
        "fallback" => fallback: Option<ValueExpr>,
        "sensitivity" => sensitivity: Option<Sensitivity>,
    }
}

object_type! {
    pub struct UnionType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "variants" => variants: OrderedMap<NodeName, Variant>,
    }
}

object_type! {
    pub struct Variant {
        "type" => ty: TypeExpression,
        "prefix" => prefix: Option<String>,
    }
}

object_type! {
    pub struct MapType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "key-type" => key_type: TypeExpression,
        "value-type" => value_type: TypeExpression,
    }
}

object_type! {
    /// A pattern codec declaration: one whole-string match, the named
    /// captures it types, and the output type a decoded value carries.
    pub struct CodecType {
        "name" => name: TypeName,
        "parameters" => parameters: Option<Vec<TypeName>>,
        "output" => output: TypeExpression,
        "match" => r#match: String,
        "captures" => captures: OrderedMap<MemberName, Capture>,
        "namespace" => namespace: Option<MemberName>,
    }
}

object_type! {
    /// The capture's name is the key of the enclosing ordered map.
    pub struct Capture {
        "type" => ty: TypeExpression,
    }
}

object_type! {
    /// An include directive among a declaring container's like members.
    pub struct IncludeRef {
        "target" => target: TypeName,
    }
}

union_type! {
    pub enum Constraint {
        "all-or-none" => AllOrNone(AllOrNone),
        "requires" => Requires(Requires),
    }
}

object_type! {
    pub struct AllOrNone {
        "fields" => fields: Vec<FieldName>,
    }
}

object_type! {
    pub struct Requires {
        "field" => field: FieldName,
        "required" => required: FieldName,
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    TypeName::SHAPE,
    NodeName::SHAPE,
    FieldName::SHAPE,
    MemberName::SHAPE,
    ValidatorName::SHAPE,
    TypeExpression::SHAPE,
    BuiltinType::SHAPE,
    TypeConstructor::SHAPE,
    StringFormat::SHAPE,
    Sensitivity::SHAPE,
    TypeDef::SHAPE,
    StringType::SHAPE,
    EnumType::SHAPE,
    EnumMember::SHAPE,
    ObjectType::SHAPE,
    FieldDef::SHAPE,
    UnionType::SHAPE,
    Variant::SHAPE,
    MapType::SHAPE,
    CodecType::SHAPE,
    Capture::SHAPE,
    IncludeRef::SHAPE,
    Constraint::SHAPE,
    AllOrNone::SHAPE,
    Requires::SHAPE,
];
