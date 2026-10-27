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
    pub struct ProjectionName;
}

string_type! {
    pub struct HandlerName;
}

string_type! {
    pub struct LayerName;
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
        // How a second value reaching a slot that already holds one is
        // treated when the slot does not accumulate.
        "duplicates" => duplicates: SlotDuplicates,
        // The layer order of a layered slot, highest precedence first.
        "layers" => layers: Option<Vec<LayerName>>,
    }
}

impl SlotDef {
    /// The identity domain its declarations register in.
    pub const DOMAIN: &str = "SlotDef";
}

enum_type! {
    /// How a set effect reaching an occupied slot is treated; the
    /// fail-closed default rejects.
    pub enum SlotDuplicates {
        "reject" => Reject,
        "reject-different" => RejectDifferent,
        "last-wins" => LastWins,
        "layered" => Layered,
    }
}

object_type! {
    pub struct AccumulationPolicy {
        "op" => op: AccumulateOp,
        // How merge treats a key contributed twice.
        "duplicates" => duplicates: MergeDuplicates,
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
    /// How merge treats a key the held map already carries; the
    /// fail-closed default rejects.
    pub enum MergeDuplicates {
        "reject" => Reject,
        "last-wins" => LastWins,
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
        // The group's set effects replace the held value of the slots
        // they name instead of failing the duplicate-set check.
        "replace" => replace: Option<bool>,
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
        // The layer a set on a layered slot names.
        "layer" => layer: Option<LayerName>,
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
    ProjectionName::SHAPE,
    HandlerName::SHAPE,
    LayerName::SHAPE,
    SlotDef::SHAPE,
    SlotDuplicates::SHAPE,
    AccumulationPolicy::SHAPE,
    AccumulateOp::SHAPE,
    MergeDuplicates::SHAPE,
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
