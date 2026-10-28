//! Node declarations contributed by `bindings.kdl`: node declarations,
//! field bindings, binding sources, and child rules.

use kdlith_engine::NodeShape;

const SOURCE_PARENTS: &[&str] = &["from", "child", "children"];

pub(crate) const NODES: &[NodeShape] = &[
    NodeShape::new("node")
        .under(&["schema", "dialect"])
        .declaration()
        .ty("NodeDef"),
    NodeShape::new("annotation")
        .under(&["schema"])
        .declaration()
        .ty("AnnotationDef"),
    NodeShape::new("group")
        .under(&["schema"])
        .declaration()
        .ty("NodeGroupDef"),
    NodeShape::new("field")
        .under(&["node", "case"])
        .ty("FieldBinding"),
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
    NodeShape::new("children-group")
        .under(&["from"])
        .ty("ChildrenGroupSource"),
    NodeShape::new("child-group")
        .under(&["from"])
        .ty("ChildGroupSource"),
    NodeShape::new("ordinal")
        .under(&["from"])
        .ty("OrdinalSource"),
    NodeShape::new("parent").under(&["from"]).ty("ParentSource"),
    NodeShape::new("children")
        .under(&["node", "case"])
        .ty("ChildRule"),
    NodeShape::new("children-group")
        .under(&["node"])
        .ty("ChildRule"),
    NodeShape::new("when")
        .under(&["children", "children-group"])
        .ty("PropertyCondition"),
    NodeShape::new("when-all")
        .under(&["children", "children-group"])
        .ty("ConditionGroup"),
    NodeShape::new("when-any")
        .under(&["children", "children-group"])
        .ty("ConditionGroup"),
    NodeShape::new("when")
        .under(&["when-all", "when-any"])
        .ty("PropertyCondition"),
    NodeShape::new("when-all")
        .under(&["when-all", "when-any"])
        .ty("ConditionGroup"),
    NodeShape::new("when-any")
        .under(&["when-all", "when-any"])
        .ty("ConditionGroup"),
    NodeShape::new("value")
        .under(&["node"])
        .ty("DependentBinding"),
    NodeShape::new("against")
        .under(&["value"])
        .ty("DependentTarget"),
    NodeShape::new("value-from-path")
        .under(&["node"])
        .ty("PathValueBinding"),
    NodeShape::new("cases").under(&["node"]).ty("CaseDispatch"),
    NodeShape::new("case").under(&["cases"]).ty("CaseRule"),
    NodeShape::new("otherwise")
        .under(&["cases"])
        .ty("Otherwise"),
    NodeShape::new("annotation")
        .under(&["node"])
        .ty("NodeAnnotation"),
];
