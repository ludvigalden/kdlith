//! Node declarations contributed by `documents.kdl`: definition roots,
//! containers, structural rules, imports, and the deferral marker.

use kdlith_engine::NodeShape;

pub(crate) const NODES: &[NodeShape] = &[
    NodeShape::new("dialect").container().ty("DialectDef"),
    NodeShape::new("schema").container().ty("SchemaDef"),
    NodeShape::new("types")
        .under(&["dialect", "schema"])
        .container(),
    NodeShape::new("document")
        .under(&["dialect", "schema"])
        .record()
        .ty("DocumentRule"),
    NodeShape::new("first").under(&["document"]).ty("FirstRule"),
    NodeShape::new("exactly-one")
        .under(&["node"])
        .ty("ExactlyOneGroup"),
    NodeShape::new("import")
        .under(&["dialect", "schema"])
        .source_import()
        .ty("ImportRequest"),
    NodeShape::new("deferred")
        .under(&["dialect"])
        .declaration()
        .ty("Deferral"),
];
