//! Node declarations contributed by `context.kdl`: context containers,
//! slots, effect groups, and effects.

use kdlith_engine::NodeShape;

pub(crate) const NODES: &[NodeShape] = &[
    NodeShape::new("context")
        .under(&["schema", "dialect"])
        .container(),
    NodeShape::new("slot")
        .under(&["context"])
        .declaration()
        .ty("SlotDef"),
    NodeShape::new("accumulate")
        .under(&["slot"])
        .ty("AccumulationPolicy"),
    NodeShape::new("source-context")
        .under(&["context"])
        .record()
        .ty("SourceContextDef"),
    NodeShape::new("field")
        .under(&["source-context"])
        .ty("SourceFieldDef"),
    NodeShape::new("path-stem")
        .under(&["from"])
        .ty("PathStemSource"),
    NodeShape::new("effects").under(&["node"]).ty("EffectGroup"),
    NodeShape::new("set").under(&["effects"]).ty("SetEffect"),
    NodeShape::new("remove")
        .under(&["effects"])
        .ty("RemoveEffect"),
    NodeShape::new("remove-from-args")
        .under(&["effects"])
        .ty("RemoveArgsEffect"),
    NodeShape::new("clear")
        .under(&["effects"])
        .ty("ClearEffect"),
    NodeShape::new("handler")
        .under(&["effects"])
        .ty("HandlerEffect"),
];
