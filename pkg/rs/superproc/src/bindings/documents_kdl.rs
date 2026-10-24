//! Node declarations contributed by `documents.kdl`: definition roots,
//! containers, structural rules, and imports.

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
    NodeShape::new("profile")
        .under(&["schema"])
        .declaration()
        .ty("ProfileDef"),
    NodeShape::new("roots").under(&["profile"]).ty("RootsRule"),
    NodeShape::new("match")
        .under(&["profile"])
        .ty("ProfileMatch"),
    NodeShape::new("path-suffix")
        .under(&["match"])
        .ty("PathSuffixMatch"),
    NodeShape::new("path-prefix")
        .under(&["match"])
        .ty("PathPrefixMatch"),
    NodeShape::new("file-name")
        .under(&["match"])
        .ty("FileNameMatch"),
    NodeShape::new("descendants")
        .under(&["node"])
        .ty("DescendantRule"),
    NodeShape::new("sequence")
        .under(&["node"])
        .ty("SequenceRule"),
    NodeShape::new("step")
        .under(&["sequence"])
        .ty("SequenceStep"),
    NodeShape::new("alternatives")
        .under(&["step"])
        .ty("AlternativesRule"),
    NodeShape::new("locations")
        .under(&["schema"])
        .record()
        .ty("LocationsRule"),
    NodeShape::new("locations")
        .under(&["profile"])
        .ty("LocationsRule"),
    NodeShape::new("patterns")
        .under(&["locations"])
        .ty("PatternsRule"),
    NodeShape::new("identity")
        .under(&["node"])
        .ty("IdentityBlock"),
    NodeShape::new("domain")
        .under(&["identity"])
        .ty("DomainDecl"),
    NodeShape::new("compose")
        .under(&["identity"])
        .ty("ComposeRule"),
    NodeShape::new("reference")
        .under(&["schema"])
        .declaration()
        .ty("ReferenceDef"),
    NodeShape::new("qualifier")
        .under(&["reference"])
        .ty("QualifierRule"),
    NodeShape::new("resolution")
        .under(&["schema"])
        .declaration()
        .ty("ResolutionPolicyDef"),
    NodeShape::new("provider")
        .under(&["schema"])
        .declaration()
        .ty("SecretProvider"),
];
