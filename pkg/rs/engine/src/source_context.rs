//! Source context: fields derived from source metadata, applied under
//! their declared mode before the document's first semantic node, with
//! every derived value's origin recorded in provenance.

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{Location, OriginId, Position, SourceId, Span, Value};
use kdlith_runtime::{Context, ContextEffect, SlotValue};

use crate::decode::Decoder;
use crate::definition::{Definition, SourceContextMode};
use crate::extensions::PATH_STEM_SOURCE;
use crate::names::Names;
use crate::types::TypeExpr;

/// The code a derived value that fails its declared type reports under.
const VALUE_TYPE: &str = "KDLITH-BND003";

/// The nowhere location a source-metadata value carries: the value is
/// derived from the file's identity, not from any span inside it.
fn nowhere() -> Location {
    Location::new(
        SourceId(0),
        Span::new(Position::new(0, 1, 1), Position::new(0, 1, 1)),
    )
}

/// One derived value and the slot it landed in.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppliedSourceValue {
    /// The slot the value was set into, by name.
    pub slot: String,
    pub value: Value,
}

/// Apply the definition's source-context block to `context`, deriving
/// each field's value from `path` under its registered providers. Values
/// are validated against both the field's declared type and the slot's;
/// a value that fails either is reported and the slot is left as it
/// was. The mode governs what the derived value may replace: `initial`
/// and `fixed` set the slot, `fallback` sets only a slot that is unset.
/// Diagnostics carry the source-metadata origin, so filename inference
/// never becomes an invisible source of precedence.
pub fn apply(
    definition: &Definition,
    context: &mut Context,
    path: &str,
) -> (Vec<AppliedSourceValue>, Vec<DiagnosticRecord>) {
    let mut applied = Vec::new();
    let mut records = Vec::new();
    let Some(decl) = &definition.source_context else {
        return (applied, records);
    };
    let (decoder, bad) = Decoder::new(&definition.types);
    if !bad.is_empty() {
        // A pattern that does not compile is a definition fault the
        // complete checks report; here it only means no value can be
        // validated, so nothing is applied.
        let record = DiagnosticRecord::from_code("KDLITH-DLC005")
            .expect("KDLITH-DLC005 is registered and never retired")
            .with_message(format!(
                "a declared pattern does not compile: {}",
                bad.first().map_or("", |b| &b.reason)
            ))
            .with_origin("engine");
        records.push(record);
        return (applied, records);
    }
    let stem = file_stem(path);
    let slot_type = |name| {
        definition
            .slots
            .iter()
            .find(|slot| slot.name == name)
            .map(|slot| slot.ty.clone())
    };
    for field in &decl.fields {
        let Some(slot) = definition.plan.context.slot(field.name) else {
            continue;
        };
        if context.get(slot).is_some() && decl.mode == SourceContextMode::Fallback {
            continue;
        }
        let Some(value) = derive(&field.sources, &stem) else {
            continue;
        };
        let mut checks = vec![field.ty.clone()];
        checks.extend(slot_type(field.name));
        let name = definition.names.resolve(field.name).to_owned();
        if !checks.iter().all(|ty| {
            decoder
                .decode(&definition.types, &definition.names, ty, &value, nowhere())
                .is_some()
        }) {
            let ty = checks.last().cloned().unwrap_or_else(|| field.ty.clone());
            records.push(value_type_record(&name, &ty, &value, &definition.names));
            continue;
        }
        if context
            .apply(ContextEffect::Set {
                slot,
                value: SlotValue::new(value.clone(), OriginId::SOURCE_METADATA),
            })
            .is_err()
        {
            continue;
        }
        applied.push(AppliedSourceValue { slot: name, value });
    }
    (applied, records)
}

/// The derived value of one field from its registered sources. Only the
/// `path-stem` provider is defined; a source list naming anything else
/// cannot compile, so reaching here with an unknown name yields nothing.
fn derive(sources: &[String], stem: &str) -> Option<Value> {
    match sources.first()?.as_str() {
        PATH_STEM_SOURCE => Some(Value::String(stem.to_owned())),
        _ => None,
    }
}

/// A derived value that failed its declared type.
fn value_type_record(slot: &str, ty: &TypeExpr, value: &Value, names: &Names) -> DiagnosticRecord {
    let expected = crate::summary::expr(names, ty);
    let found = crate::decode::found(value);
    DiagnosticRecord::from_code(VALUE_TYPE)
        .expect("KDLITH-BND003 is registered and never retired")
        .with_message(format!(
            "source-context {slot} expects {expected}, found {found}"
        ))
        .with_origin("source-metadata")
}

/// The file name's stem, without extension: `dir/file.name.kdl` yields
/// `file.name`. A path with no file component yields the empty stem,
/// which the field's type rejects if it does not accept it.
fn file_stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match file.rsplit_once('.') {
        Some((stem, _)) => stem.to_owned(),
        None => file.to_owned(),
    }
}
