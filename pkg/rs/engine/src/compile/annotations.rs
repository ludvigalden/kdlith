//! Annotation checks: every node declaration's annotation facts are
//! checked against the schema's `annotation` declarations. A declared
//! type that is not one scalar value, a fact naming no declared
//! annotation, one annotation carried twice by one node, a value that
//! does not decode as its declared type, and a node declaration missing
//! a required annotation are malformed constructs (`KDLITH-DLC005`). A
//! declared type naming no type is an unknown type like any other.
//! Annotations are metadata: nothing here changes binding.

use kdlith_model::{Location, Position, SourceId, Span};

use crate::bindings::BindingCatalog;
use crate::compile::Cx;
use crate::compile::link::unresolved;
use crate::decode::{self, Decoder};
use crate::diag::Site;
use crate::names::Sym;
use crate::summary;
use crate::types::{Builtin, TypeExpr, TypeKind};

/// Whether a declared annotation type holds one scalar value: a built-in
/// scalar other than `ValueExpr`, or a declared string or enum type.
fn scalar(cx: &Cx<'_>, ty: &TypeExpr) -> bool {
    let TypeExpr::Named(name) = ty else {
        return false;
    };
    let kind = cx
        .types
        .lookup(*name)
        .and_then(|id| cx.types.get(id))
        .map(|decl| &decl.kind);
    matches!(
        kind,
        Some(
            TypeKind::Builtin(
                Builtin::String | Builtin::Bool | Builtin::Integer | Builtin::Decimal | Builtin::U64
            ) | TypeKind::String(_)
                | TypeKind::Enum(_)
        )
    )
}

/// Run every annotation check, after linking has resolved the declared
/// type names.
pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    let mut found: Vec<(Site, String, String)> = Vec::new();
    let mut unknown: Vec<(Site, String)> = Vec::new();
    let mut usable: Vec<Sym> = Vec::new();
    for decl in &cx.annotations {
        if scalar(cx, &decl.ty) {
            usable.push(decl.name);
            continue;
        }
        if let TypeExpr::Named(name) = &decl.ty
            && cx.types.lookup(*name).is_none()
        {
            unknown.push((decl.site, cx.names.resolve(*name).to_owned()));
            continue;
        }
        let label = format!("annotation {}", cx.names.resolve(decl.name));
        let reason = format!(
            "type {} is not a scalar type; an annotation holds one String, Bool, Integer, Decimal, U64, declared string, or declared enum value",
            summary::expr(&cx.names, &decl.ty)
        );
        found.push((decl.site, label, reason));
    }
    // Annotation values are literals of the definition itself; no source
    // location is carried into a value decoded from one.
    let nowhere = Location::new(
        SourceId(0),
        Span::new(Position::new(0, 1, 1), Position::new(0, 1, 1)),
    );
    let (decoder, _) = Decoder::new(&cx.types);
    for (_, node) in bindings.iter() {
        let label = format!("node {}", cx.names.resolve(node.name));
        let mut seen: Vec<Sym> = Vec::new();
        for fact in &node.annotations {
            let name = cx.names.resolve(fact.name);
            let Some(decl) = cx.annotations.iter().find(|decl| decl.name == fact.name) else {
                let reason = format!("annotation {name} is not declared");
                found.push((node.site, label.clone(), reason));
                continue;
            };
            if seen.contains(&fact.name) {
                let reason = format!("carries annotation {name} more than once");
                found.push((node.site, label.clone(), reason));
                continue;
            }
            seen.push(fact.name);
            if usable.contains(&decl.name)
                && decoder
                    .decode(&cx.types, &cx.names, &decl.ty, &fact.value, nowhere)
                    .is_none()
            {
                let reason = format!(
                    "annotation {name} holds {}, which does not decode as {}",
                    decode::found(&fact.value),
                    summary::expr(&cx.names, &decl.ty)
                );
                found.push((node.site, label.clone(), reason));
            }
        }
        for decl in cx.annotations.iter().filter(|decl| decl.required) {
            if !seen.contains(&decl.name) {
                let reason = format!(
                    "is missing the required annotation {}",
                    cx.names.resolve(decl.name)
                );
                found.push((node.site, label.clone(), reason));
            }
        }
    }
    for (site, name) in unknown {
        unresolved(cx, site, "type", &name);
    }
    for (site, label, reason) in found {
        cx.malformed(site, &label, &reason);
    }
}
