//! Logical types contributed by `context.kdl`: typed slots, effect
//! groups and targets, effects, context policies, source context,
//! accumulating context, and context snapshots.

use kdlith_engine::{
    OrderedMap, Shaped, TypeShape, ValueExpr, enum_type, object_type, string_type, union_type,
};

use super::{FieldName, TypeExpression, TypeName};

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
        // The slot's accumulation policy: how an accumulating effect
        // combines with the value already held.
        "accumulate" => accumulate: Option<AccumulationPolicy>,
    }
}

object_type! {
    pub struct AccumulationPolicy {
        "op" => op: AccumulateOp,
    }
}

enum_type! {
    /// How an accumulating effect combines with the slot's incoming
    /// value: append concatenates lists; merge combines maps key-wise.
    pub enum AccumulateOp {
        "append" => Append,
        "merge" => Merge,
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

object_type! {
    /// Fields derived from source metadata before the document's first
    /// semantic node; the mode is explicit, with no default.
    pub struct SourceContextDef {
        "mode" => mode: SourceContextMode,
        "fields" => fields: OrderedMap<FieldName, SourceFieldDef>,
    }
}

enum_type! {
    pub enum SourceContextMode {
        "initial" => Initial,
        "fallback" => Fallback,
        "fixed" => Fixed,
    }
}

object_type! {
    pub struct SourceFieldDef {
        "type" => ty: TypeExpression,
        "sources" => sources: Vec<PathStemSource>,
    }
}

object_type! {
    /// The path-stem source-metadata provider.
    pub struct PathStemSource {
    }
}

object_type! {
    /// One entry of the index the engine writes while scanning, so a
    /// record can be resolved later by restoring its context snapshot.
    pub struct SnapshotIndexEntry {
        "identity" => identity: String,
        "type" => ty: TypeName,
        "digest" => digest: String,
        "span" => span: String,
        "schema-fingerprint" => schema_fingerprint: String,
        "snapshot" => snapshot: String,
        "invocation" => invocation: String,
        "validation" => validation: ValidationStatus,
    }
}

enum_type! {
    pub enum ValidationStatus {
        "validated" => Validated,
        "pending" => Pending,
        "failed" => Failed,
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    SlotName::SHAPE,
    NamespaceName::SHAPE,
    HandlerName::SHAPE,
    SlotDef::SHAPE,
    AccumulationPolicy::SHAPE,
    AccumulateOp::SHAPE,
    EffectTarget::SHAPE,
    EffectGroup::SHAPE,
    Effect::SHAPE,
    SetEffect::SHAPE,
    RemoveEffect::SHAPE,
    ClearEffect::SHAPE,
    HandlerEffect::SHAPE,
    ContextPolicy::SHAPE,
    SourceContextDef::SHAPE,
    SourceContextMode::SHAPE,
    SourceFieldDef::SHAPE,
    PathStemSource::SHAPE,
    SnapshotIndexEntry::SHAPE,
    ValidationStatus::SHAPE,
];
