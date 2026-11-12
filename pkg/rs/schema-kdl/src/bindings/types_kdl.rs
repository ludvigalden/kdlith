//! Node declarations contributed by `types.kdl`: how type declarations
//! are spelled in a definition.

use kdlith_engine::NodeShape;

pub(crate) const NODES: &[NodeShape] = &[
    NodeShape::new("string")
        .under(&["types"])
        .declaration()
        .ty("StringType"),
    NodeShape::new("enum")
        .under(&["types"])
        .declaration()
        .ty("EnumType"),
    NodeShape::new("member").under(&["enum"]).ty("EnumMember"),
    NodeShape::new("object")
        .under(&["types"])
        .declaration()
        .ty("ObjectType"),
    NodeShape::new("field").under(&["object"]).ty("FieldDef"),
    NodeShape::new("fallback").under(&["field"]),
    NodeShape::new("constraints").under(&["object"]),
    NodeShape::new("all-or-none")
        .under(&["constraints"])
        .ty("AllOrNone"),
    NodeShape::new("requires")
        .under(&["constraints"])
        .ty("Requires"),
    NodeShape::new("bound").under(&["constraints"]).ty("Bound"),
    NodeShape::new("at-most-one")
        .under(&["constraints"])
        .ty("AtMostOne"),
    NodeShape::new("exactly-one-field")
        .under(&["constraints"])
        .ty("ExactlyOneField"),
    NodeShape::new("union")
        .under(&["types"])
        .declaration()
        .ty("UnionType"),
    NodeShape::new("variant").under(&["union"]).ty("Variant"),
    NodeShape::new("map")
        .under(&["types"])
        .declaration()
        .ty("MapType"),
    NodeShape::new("codec")
        .under(&["types"])
        .declaration()
        .ty("CodecType"),
    NodeShape::new("capture").under(&["codec"]).ty("Capture"),
    NodeShape::new("asset")
        .under(&["types"])
        .declaration()
        .ty("AssetType"),
    NodeShape::new("include")
        .under(&["object", "enum", "union", "codec"])
        .ty("IncludeRef"),
];
