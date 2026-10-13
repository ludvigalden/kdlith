//! Linking: every type, node, parent, field, slot, and namespace a
//! definition references must be declared, and every `ref<T>` must
//! target a declared type; children effects must read header fields.

use crate::bindings::{BindingCatalog, Effect, EffectTarget, NodeDecl, Placement, SetFrom, Source};
use crate::compile::Cx;
use crate::definition::DefinitionKind;
use crate::diag::{
    NOT_STREAMABLE, Site, TYPE_EXPRESSION, UNKNOWN_TYPE, UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA,
};
use crate::names::Sym;
use crate::symbols::SymbolCatalog;
use crate::types::{TypeExpr, TypeKind};

pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog, symbols: &SymbolCatalog) {
    types(cx, symbols);
    for slot in cx.slots.clone() {
        expr(cx, symbols, slot.site, &slot.ty);
    }
    for (_, node) in bindings.iter() {
        node_refs(cx, bindings, symbols, node);
    }
    document_rule(cx, bindings);
}

/// An undeclared reference, reported in the definition's own area.
fn unresolved(cx: &mut Cx<'_>, site: Site, kind: &str, name: &str) {
    match cx.kind {
        DefinitionKind::Dialect => cx.diags.error(UNRESOLVED_DIALECT, site, &[kind, name]),
        DefinitionKind::Schema if kind == "type" => cx.diags.error(UNKNOWN_TYPE, site, &[name]),
        DefinitionKind::Schema => cx.diags.error(UNRESOLVED_SCHEMA, site, &[kind, name]),
    }
}

fn types(cx: &mut Cx<'_>, symbols: &SymbolCatalog) {
    let mut found: Vec<(Site, TypeExpr)> = Vec::new();
    for (_, decl) in cx.types.iter() {
        let Some(site) = decl.site else { continue };
        match &decl.kind {
            TypeKind::Object(object) => {
                found.extend(object.fields.iter().map(|f| (site, f.ty.clone())));
            }
            TypeKind::Union(union) => {
                found.extend(union.variants.iter().map(|v| (site, v.ty.clone())));
            }
            TypeKind::Map(map) => {
                found.push((site, map.key.clone()));
                found.push((site, map.value.clone()));
            }
            _ => {}
        }
    }
    for (site, ty) in found {
        expr(cx, symbols, site, &ty);
    }
}

/// Check every name a type expression uses.
fn expr(cx: &mut Cx<'_>, symbols: &SymbolCatalog, site: Site, ty: &TypeExpr) {
    match ty {
        TypeExpr::Named(name) => {
            named(cx, site, *name);
        }
        TypeExpr::Ref(inner) => {
            let TypeExpr::Named(target) = inner.as_ref() else {
                cx.diags.error(
                    TYPE_EXPRESSION,
                    site,
                    &["ref<...>", "ref takes a declared type name"],
                );
                return;
            };
            if !named(cx, site, *target) {
                return;
            }
            if !symbols.declares(*target) {
                let spelled = cx.names.resolve(*target).to_owned();
                unresolved(cx, site, "declaration of reference target", &spelled);
            }
        }
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) | TypeExpr::List(inner) => {
            expr(cx, symbols, site, inner);
        }
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            expr(cx, symbols, site, key);
            expr(cx, symbols, site, value);
        }
        TypeExpr::Union(members) => {
            for member in members {
                expr(cx, symbols, site, member);
            }
        }
    }
}

fn named(cx: &mut Cx<'_>, site: Site, name: Sym) -> bool {
    if cx.types.lookup(name).is_some() {
        return true;
    }
    let spelled = cx.names.resolve(name).to_owned();
    unresolved(cx, site, "type", &spelled);
    false
}

fn node_refs(cx: &mut Cx<'_>, bindings: &BindingCatalog, symbols: &SymbolCatalog, node: &NodeDecl) {
    let label = cx.names.resolve(node.name).to_owned();
    let object = node
        .ty
        .and_then(|ty| object_fields(cx, node.site, &label, ty));
    if let Placement::Under(parents) = &node.placement {
        for parent in parents {
            let owned = bindings
                .scopes()
                .iter()
                .any(|scope| scope.name == Some(*parent));
            if !owned {
                let spelled = format!("{} of node {label}", cx.names.resolve(*parent));
                unresolved(cx, node.site, "parent", &spelled);
            }
        }
    }
    for field in &node.fields {
        if let Some(fields) = &object
            && !fields.contains(&field.name)
        {
            let ty = node.ty.map_or("", |ty| cx.names.resolve(ty));
            let spelled = format!("{} of type {ty}", cx.names.resolve(field.name));
            unresolved(cx, field.site, "field", &spelled);
        }
        if let Some(ty) = &field.ty {
            expr(cx, symbols, field.site, ty);
        }
        for source in &field.sources {
            if let Source::Context { slot, .. } = source {
                slot_ref(cx, field.site, *slot);
            }
        }
    }
    for group in &node.effects {
        for effect in &group.effects {
            match effect {
                Effect::Set { slot, from } => {
                    slot_ref(cx, group.site, *slot);
                    if let SetFrom::Field(field) = from {
                        effect_field(cx, node, &label, group.target, group.site, (*slot, *field));
                    }
                }
                Effect::Remove { slot } => slot_ref(cx, group.site, *slot),
                Effect::Clear { namespace } => {
                    if !cx.slots.iter().any(|s| s.namespace == Some(*namespace)) {
                        let spelled = cx.names.resolve(*namespace).to_owned();
                        unresolved(cx, group.site, "context namespace", &spelled);
                    }
                }
                Effect::Handler { .. } => {}
            }
        }
    }
}

/// The field names of a node's `type=`, which must be a declared object.
fn object_fields(cx: &mut Cx<'_>, site: Site, label: &str, ty: Sym) -> Option<Vec<Sym>> {
    let decl = cx.types.lookup(ty).and_then(|id| cx.types.get(id));
    if let Some(TypeKind::Object(object)) = decl.map(|decl| &decl.kind) {
        return Some(object.fields.iter().map(|f| f.name).collect());
    }
    let spelled = format!("{} of node {label}", cx.names.resolve(ty));
    unresolved(cx, site, "object type", &spelled);
    None
}

fn slot_ref(cx: &mut Cx<'_>, site: Site, slot: Sym) {
    if !cx.slots.iter().any(|s| s.name == slot) {
        let spelled = cx.names.resolve(slot).to_owned();
        unresolved(cx, site, "context slot", &spelled);
    }
}

/// A `set field=` names a field the node binds; on a children effect,
/// that field must come from the header, not from a child.
fn effect_field(
    cx: &mut Cx<'_>,
    node: &NodeDecl,
    label: &str,
    target: EffectTarget,
    site: Site,
    (slot, field): (Sym, Sym),
) {
    let Some(binding) = node.fields.iter().find(|f| f.name == field) else {
        let spelled = format!("{} of node {label}", cx.names.resolve(field));
        unresolved(cx, site, "field", &spelled);
        return;
    };
    let from_child = binding.sources.iter().any(|source| {
        matches!(
            source,
            Source::Child { .. } | Source::Children { .. } | Source::Entries { .. }
        )
    });
    if target == EffectTarget::Children && from_child {
        let slot = cx.names.resolve(slot).to_owned();
        let field = cx.names.resolve(field).to_owned();
        cx.diags
            .error(NOT_STREAMABLE, site, &[label, &slot, &field]);
    }
}

fn document_rule(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    let Some(rule) = cx.document_rule.clone() else {
        return;
    };
    let named = rule.first.iter().flatten().chain(rule.following.iter());
    for name in named {
        let root = bindings
            .iter()
            .any(|(_, node)| node.name == *name && node.placement == Placement::Root);
        if !root {
            let spelled = cx.names.resolve(*name).to_owned();
            unresolved(cx, rule.site, "root node", &spelled);
        }
    }
}
