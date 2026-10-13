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
    NodeShape::new("effects").under(&["node"]).ty("EffectGroup"),
    NodeShape::new("set").under(&["effects"]).ty("SetEffect"),
    NodeShape::new("remove")
        .under(&["effects"])
        .ty("RemoveEffect"),
    NodeShape::new("clear")
        .under(&["effects"])
        .ty("ClearEffect"),
    NodeShape::new("handler")
        .under(&["effects"])
        .ty("HandlerEffect"),
];
