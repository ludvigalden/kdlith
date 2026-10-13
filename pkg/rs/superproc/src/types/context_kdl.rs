//! Logical types contributed by `context.kdl`: typed slots, effect
//! groups and targets, effects, and context policies.

use kdlith_engine::{
    Shaped, TypeShape, ValueExpr, enum_type, object_type, string_type, union_type,
};

use super::{FieldName, TypeExpression};

string_type! {
    pub struct SlotName;
}

string_type! {
    pub struct NamespaceName;
}

string_type! {
    pub struct HandlerName;
}

object_type! {
    pub struct SlotDef {
        "name" => name: SlotName,
        "type" => ty: TypeExpression,
        "namespace" => namespace: Option<NamespaceName>,
        "description" => description: Option<String>,
    }
}

enum_type! {
    pub enum EffectTarget {
        "following" => Following,
        "children" => Children,
    }
}

object_type! {
    pub struct EffectGroup {
        "target" => target: EffectTarget,
        "effects" => effects: Vec<Effect>,
    }
}

union_type! {
    pub enum Effect {
        "set" => Set(SetEffect),
        "remove" => Remove(RemoveEffect),
        "clear" => Clear(ClearEffect),
        "handler" => Handler(HandlerEffect),
    }
}

object_type! {
    pub struct SetEffect {
        "slot" => slot: SlotName,
        "field" => field: Option<FieldName>,
        "value" => value: Option<ValueExpr>,
    }
}

object_type! {
    pub struct RemoveEffect {
        "slot" => slot: SlotName,
    }
}

object_type! {
    pub struct ClearEffect {
        "namespace" => namespace: NamespaceName,
    }
}

object_type! {
    pub struct HandlerEffect {
        "name" => name: HandlerName,
    }
}

enum_type! {
    pub enum ContextPolicy {
        "fallback" => Fallback,
        "fixed" => Fixed,
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    SlotName::SHAPE,
    NamespaceName::SHAPE,
    HandlerName::SHAPE,
    SlotDef::SHAPE,
    EffectTarget::SHAPE,
    EffectGroup::SHAPE,
    Effect::SHAPE,
    SetEffect::SHAPE,
    RemoveEffect::SHAPE,
    ClearEffect::SHAPE,
    HandlerEffect::SHAPE,
    ContextPolicy::SHAPE,
];
