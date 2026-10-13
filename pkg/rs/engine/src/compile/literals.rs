//! Literal checks: every value a definition writes (a field's fallback
//! or documented default, a `set value=` effect) must decode as the type
//! it stands for, through the same decoder a binder uses on records.

use kdlith_model::{Location, Position, SourceId, Span, Value};

use crate::bindings::{BindingCatalog, Effect, SetFrom};
use crate::compile::Cx;
use crate::decode::{self, Decoder};
use crate::definition::DefinitionKind;
use crate::diag::{DIALECT_LITERAL, MALFORMED, SCHEMA_LITERAL, Site};
use crate::summary;
use crate::types::{TypeExpr, TypeKind};

pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    let (decoder, bad) = Decoder::new(&cx.types);
    for pattern in bad {
        let Some(decl) = cx.types.get(pattern.ty) else {
            continue;
        };
        let (Some(site), name) = (decl.site, cx.names.resolve(decl.name).to_owned()) else {
            continue;
        };
        let reason = format!("pattern does not compile: {}", pattern.reason);
        cx.diags
            .error(MALFORMED, site, &[&format!("string {name}"), &reason]);
    }
    let mut found: Vec<(Site, String, TypeExpr, Value)> = Vec::new();
    for (_, decl) in cx.types.iter() {
        let (Some(site), TypeKind::Object(object)) = (decl.site, &decl.kind) else {
            continue;
        };
        let owner = cx.names.resolve(decl.name);
        for field in &object.fields {
            let label = format!("field {owner}.{}", cx.names.resolve(field.name));
            let literals = [&field.fallback, &field.documented_default];
            for literal in literals.into_iter().flatten() {
                found.push((site, label.clone(), field.ty.clone(), literal.clone()));
            }
        }
    }
    for (_, node) in bindings.iter() {
        for group in &node.effects {
            for effect in &group.effects {
                let Effect::Set {
                    slot,
                    from: SetFrom::Value(value),
                } = effect
                else {
                    continue;
                };
                let Some(decl) = cx.slots.iter().find(|s| s.name == *slot) else {
                    continue;
                };
                let label = format!(
                    "node {} set {}",
                    cx.names.resolve(node.name),
                    cx.names.resolve(*slot)
                );
                found.push((group.site, label, decl.ty.clone(), value.clone()));
            }
        }
    }
    let code = match cx.kind {
        DefinitionKind::Dialect => DIALECT_LITERAL,
        DefinitionKind::Schema => SCHEMA_LITERAL,
    };
    // Literals are scalars of the definition itself; no source location
    // is carried into a reference decoded from one.
    let nowhere = Location::new(
        SourceId(0),
        Span::new(Position::new(0, 1, 1), Position::new(0, 1, 1)),
    );
    for (site, label, ty, value) in found {
        if decoder
            .decode(&cx.types, &cx.names, &ty, &value, nowhere)
            .is_none()
        {
            let expected = summary::expr(&cx.names, &ty);
            cx.diags
                .error(code, site, &[&label, &decode::found(&value), &expected]);
        }
    }
}
