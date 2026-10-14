//! Node declarations contributed by `bindings.kdl`: node declarations,
//! field bindings, binding sources, and child rules.

use kdlith_engine::NodeShape;

const SOURCE_PARENTS: &[&str] = &["from", "child", "children"];

pub(crate) const NODES: &[NodeShape] = &[
    NodeShape::new("node")
        .under(&["schema", "dialect"])
        .declaration()
        .ty("NodeDef"),
    NodeShape::new("field").under(&["node"]).ty("FieldBinding"),
    NodeShape::new("argument")
        .under(SOURCE_PARENTS)
        .ty("ArgumentSource"),
    NodeShape::new("args")
        .under(SOURCE_PARENTS)
        .ty("ArgsSource"),
    NodeShape::new("property")
        .under(SOURCE_PARENTS)
        .ty("PropertySource"),
    NodeShape::new("child")
        .under(SOURCE_PARENTS)
        .ty("ChildSource"),
    NodeShape::new("children")
        .under(SOURCE_PARENTS)
        .ty("ChildrenSource"),
    NodeShape::new("entries")
        .under(&["from"])
        .ty("EntriesSource"),
    NodeShape::new("expand").under(&["from"]).ty("ExpandSource"),
    NodeShape::new("context")
        .under(&["from"])
        .ty("ContextSource"),
    NodeShape::new("children").under(&["node"]).ty("ChildRule"),
    NodeShape::new("value")
        .under(&["node"])
        .ty("DependentBinding"),
    NodeShape::new("against")
        .under(&["value"])
        .ty("DependentTarget"),
    NodeShape::new("cases").under(&["node"]).ty("CaseDispatch"),
    NodeShape::new("case").under(&["cases"]).ty("CaseRule"),
    NodeShape::new("otherwise")
        .under(&["cases"])
        .ty("Otherwise"),
];
