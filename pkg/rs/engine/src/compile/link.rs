//! Linking: every type, node, parent, field, slot, and namespace a
//! definition references must be declared, prefixes must be
//! unambiguous, and dependent targets must resolve.

use crate::bindings::{
    BindingCatalog, Effect, EffectTarget, NodeDecl, NodeDeclId, Placement, SetFrom, Source,
};
use crate::compile::Cx;
use crate::definition::{DefinitionKind, SlotDuplicates};
use crate::diag::{
    DISPATCH_PREFIX, FALLBACK_IDENTITY, INHERITANCE_CYCLE, NOT_STREAMABLE, Site, TYPE_EXPRESSION,
    UNKNOWN_TYPE, UNRESOLVED_DIALECT, UNRESOLVED_SCHEMA,
};
use crate::names::Sym;
use crate::summary::expr as expr_name;
use crate::symbols::SymbolCatalog;
use crate::types::{Builtin, TypeExpr, TypeKind};

pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog, symbols: &SymbolCatalog) {
    types(cx, symbols);
    references(cx, symbols);
    prefixes(cx);
    expansions(cx, bindings);
    for slot in cx.slots.clone() {
        expr(cx, symbols, slot.site, &slot.ty, &[]);
    }
    for (id, node) in bindings.iter() {
        node_refs(cx, bindings, symbols, node);
        fallback_identity(cx, symbols, id, node);
    }
    profiles(cx);
    document_rule(cx, bindings);
}

/// A profile's `extends` names a profile the definition declares, and
/// no `extends` chain returns to a profile already on it. Each cycle is
/// reported once, at the first-declared profile on it.
fn profiles(cx: &mut Cx<'_>) {
    let declared = |cx: &Cx<'_>, name: Sym| cx.profiles.iter().position(|p| p.name == name);
    for at in 0..cx.profiles.len() {
        let profile = &cx.profiles[at];
        if let Some(parent) = profile.extends
            && declared(cx, parent).is_none()
        {
            let (site, name) = (profile.site, profile.name);
            let spelled = format!(
                "{} extended by profile {}",
                cx.names.resolve(parent),
                cx.names.resolve(name)
            );
            unresolved(cx, site, "profile", &spelled);
        }
    }
    let mut reported: Vec<usize> = Vec::new();
    for start in 0..cx.profiles.len() {
        let mut chain = vec![start];
        let mut next = cx.profiles[start]
            .extends
            .and_then(|parent| declared(cx, parent));
        while let Some(at) = next {
            if let Some(from) = chain.iter().position(|seen| *seen == at) {
                let cycle = &chain[from..];
                let first = cycle.iter().copied().min().unwrap_or(at);
                if !reported.contains(&first) {
                    reported.extend_from_slice(cycle);
                    let rotation = cycle.iter().position(|i| *i == first).unwrap_or(0);
                    let mut spelled: Vec<&str> = cycle[rotation..]
                        .iter()
                        .chain(&cycle[..rotation])
                        .map(|i| cx.names.resolve(cx.profiles[*i].name))
                        .collect();
                    spelled.push(cx.names.resolve(cx.profiles[first].name));
                    let shown = spelled.join(" -> ");
                    let site = cx.profiles[first].site;
                    cx.diags.error(INHERITANCE_CYCLE, site, &[&shown]);
                }
                break;
            }
            chain.push(at);
            next = cx.profiles[at]
                .extends
                .and_then(|parent| declared(cx, parent));
        }
    }
}

/// An undeclared reference, reported in the definition's own area.
pub(crate) fn unresolved(cx: &mut Cx<'_>, site: Site, kind: &str, name: &str) {
    match cx.kind {
        DefinitionKind::Dialect => cx.diags.error(UNRESOLVED_DIALECT, site, &[kind, name]),
        DefinitionKind::Schema if kind == "type" => cx.diags.error(UNKNOWN_TYPE, site, &[name]),
        DefinitionKind::Schema => cx.diags.error(UNRESOLVED_SCHEMA, site, &[kind, name]),
    }
}

fn types(cx: &mut Cx<'_>, symbols: &SymbolCatalog) {
    let mut found: Vec<(Site, TypeExpr, Vec<Sym>)> = Vec::new();
    for (_, decl) in cx.types.iter() {
        let Some(site) = decl.site else { continue };
        // A generic declaration's parameters are placeholders its body
        // references; they are not types to resolve.
        let params = decl.parameters.clone();
        match &decl.kind {
            TypeKind::Object(object) => {
                found.extend(
                    object
                        .fields
                        .iter()
                        .map(|f| (site, f.ty.clone(), params.clone())),
                );
            }
            TypeKind::Union(union) => {
                found.extend(
                    union
                        .variants
                        .iter()
                        .map(|v| (site, v.ty.clone(), params.clone())),
                );
            }
            TypeKind::Map(map) => {
                found.push((site, map.key.clone(), params.clone()));
                found.push((site, map.value.clone(), params.clone()));
            }
            TypeKind::Codec(codec) => {
                found.push((site, codec.output.clone(), params.clone()));
                found.extend(
                    codec
                        .captures
                        .iter()
                        .map(|c| (site, c.ty.clone(), params.clone())),
                );
            }
            _ => {}
        }
    }
    for (site, ty, params) in found {
        expr(cx, symbols, site, &ty, &params);
    }
}

/// A declared reference type names the codec that parses its spelling;
/// without one, no spelling of it could bind.
fn references(cx: &mut Cx<'_>, symbols: &SymbolCatalog) {
    for reference in symbols.references() {
        let label = format!("reference {}", cx.names.resolve(reference.name));
        let Some(codec) = reference.codec else {
            let reason = "declares no codec, so no spelling of it can bind";
            cx.malformed(reference.site, &label, reason);
            continue;
        };
        if !named(cx, reference.site, codec) {
            continue;
        }
        let kind = cx.types.lookup(codec).and_then(|id| cx.types.get(id));
        if !matches!(kind.map(|decl| &decl.kind), Some(TypeKind::Codec(_))) {
            let spelled = cx.names.resolve(codec);
            let reason = format!("codec {spelled} is not a codec type");
            cx.malformed(reference.site, &label, &reason);
        }
    }
}

/// The variant prefixes of one union are pairwise unambiguous: one
/// being a prefix of another leaves value dispatch non-deterministic.
fn prefixes(cx: &mut Cx<'_>) {
    for (_, decl) in cx.types.iter() {
        let TypeKind::Union(union) = &decl.kind else {
            continue;
        };
        let Some(site) = decl.site else {
            continue;
        };
        let prefixed: Vec<&crate::types::Variant> = union
            .variants
            .iter()
            .filter(|v| v.prefix.is_some())
            .collect();
        for (at, variant) in prefixed.iter().enumerate() {
            for other in &prefixed[..at] {
                let (a, b) = (
                    variant.prefix.as_deref().unwrap_or(""),
                    other.prefix.as_deref().unwrap_or(""),
                );
                if a != b && (a.starts_with(b) || b.starts_with(a)) {
                    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
                    let values = [
                        short,
                        cx.names.resolve(other.discriminator),
                        long,
                        cx.names.resolve(variant.discriminator),
                    ];
                    cx.diags.error(DISPATCH_PREFIX, site, &values);
                }
            }
        }
    }
}

/// Every `expand` source names a declared type and one of its keyed
/// members.
fn expansions(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    for (_, node) in bindings.iter() {
        for field in &node.fields {
            for source in &field.sources {
                let Source::Expand { target, member } = source else {
                    continue;
                };
                let site = field.site;
                let spelled = cx.names.resolve(*target).to_owned();
                let Some(decl) = cx.types.lookup(*target).and_then(|id| cx.types.get(id)) else {
                    unresolved(cx, site, "type", &spelled);
                    continue;
                };
                let member = cx.names.resolve(*member).to_owned();
                let keyed = match &decl.kind {
                    TypeKind::Object(_) => "fields",
                    TypeKind::Enum(_) => "members",
                    TypeKind::Union(_) => "variants",
                    TypeKind::Codec(_) => "captures",
                    _ => "",
                };
                if member != keyed {
                    let label = format!("expand {spelled}");
                    let reason = if keyed.is_empty() {
                        format!("type {spelled} declares no keyed member")
                    } else {
                        format!("type {spelled} declares no keyed member {member} (it has {keyed})")
                    };
                    cx.malformed(site, &label, &reason);
                }
            }
        }
    }
}

/// Check every name a type expression uses. `params` are the owning
/// declaration's type parameters, which the expression may reference
/// without a declaration.
fn expr(cx: &mut Cx<'_>, symbols: &SymbolCatalog, site: Site, ty: &TypeExpr, params: &[Sym]) {
    match ty {
        TypeExpr::Named(name) => {
            // A declared reference type types a field directly.
            if params.contains(name) || symbols.reference(*name).is_some() {
                return;
            }
            named(cx, site, *name);
        }
        // Complete checking monomorphizes every instantiation first, so
        // one surviving here names its head for the same check.
        TypeExpr::Instantiation(head, args) => {
            named(cx, site, *head);
            for arg in args {
                expr(cx, symbols, site, arg, params);
            }
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
            expr(cx, symbols, site, inner, params);
        }
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            expr(cx, symbols, site, key, params);
            expr(cx, symbols, site, value, params);
        }
        TypeExpr::Union(members) => {
            for member in members {
                expr(cx, symbols, site, member, params);
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
    // A declaration's profile= names a profile the definition declares;
    // an unknown name would make the declaration unreachable under every
    // invocation, so it is an undeclared reference like any other.
    if let Some(profile) = node.profile
        && !cx.profiles.iter().any(|declared| declared.name == profile)
    {
        let spelled = format!("{} of node {label}", cx.names.resolve(profile));
        unresolved(cx, node.site, "profile", &spelled);
    }
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
            expr(cx, symbols, field.site, ty, &[]);
        }
        for source in &field.sources {
            if let Source::Context { slot, .. } = source {
                slot_ref(cx, field.site, *slot);
            }
        }
    }
    if let Some(value) = &node.value {
        dependent(cx, node, &label, value);
    }
    for case in &node.cases {
        let Some(binding) = node.fields.iter().find(|f| f.name == case.property) else {
            let spelled = format!("{} of node {label}", cx.names.resolve(case.property));
            unresolved(cx, case.site, "field", &spelled);
            continue;
        };
        let from_property = binding
            .sources
            .iter()
            .any(|s| matches!(s, Source::Property(_)));
        if !from_property {
            let spelled = cx.names.resolve(case.property).to_owned();
            cx.malformed(
                case.site,
                &label,
                &format!("case property {spelled} is not bound from a property"),
            );
        }
        expr(cx, symbols, case.site, &case.ty, &[]);
    }
    if let Some(ty) = &node.otherwise {
        expr(cx, symbols, node.site, ty, &[]);
    }
    for group in &node.effects {
        for effect in &group.effects {
            match effect {
                Effect::Set { slot, from, layer } => {
                    slot_ref(cx, group.site, *slot);
                    set_layer(cx, &label, group.site, *slot, *layer);
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

/// A declaration without an identity block names itself by argument 0's
/// field; binding rejects any value of it that is not a string, so a
/// field type admitting one is noted where the declaration is written.
fn fallback_identity(cx: &mut Cx<'_>, symbols: &SymbolCatalog, id: NodeDeclId, node: &NodeDecl) {
    let Some(declaration) = symbols.declaration(id) else {
        return;
    };
    if declaration.identity.is_some() {
        return;
    }
    let Some(field) = declaration.identity_fields.first().copied() else {
        return;
    };
    let declared = node
        .ty
        .and_then(|ty| match &cx.types.get(cx.types.lookup(ty)?)?.kind {
            TypeKind::Object(object) => object.fields.iter().find(|f| f.name == field),
            _ => None,
        });
    let ty = match declared {
        Some(declared) => Some(declared.ty.clone()),
        None => node
            .fields
            .iter()
            .find(|f| f.name == field)
            .and_then(|f| f.ty.clone()),
    };
    if ty
        .as_ref()
        .is_some_and(|ty| admits_only_strings(cx, symbols, ty, 0))
    {
        return;
    }
    let spelled = ty.map_or_else(|| "untyped".to_owned(), |ty| expr_name(&cx.names, &ty));
    let node_name = cx.names.resolve(node.name).to_owned();
    let field_name = cx.names.resolve(field).to_owned();
    cx.diags.note(
        FALLBACK_IDENTITY,
        node.site,
        &[&node_name, &field_name, &spelled],
    );
}

/// Whether every value `ty` decodes to is a string.
fn admits_only_strings(cx: &Cx<'_>, symbols: &SymbolCatalog, ty: &TypeExpr, depth: u8) -> bool {
    // A union or codec chain this deep is cyclic; leave it unnoted.
    if depth > 32 {
        return true;
    }
    let next = depth + 1;
    match ty {
        TypeExpr::Named(name) => {
            if symbols.reference(*name).is_some() {
                return false;
            }
            let Some(decl) = cx.types.lookup(*name).and_then(|id| cx.types.get(id)) else {
                return true;
            };
            match &decl.kind {
                TypeKind::Builtin(builtin) => *builtin == Builtin::String,
                TypeKind::String(_) | TypeKind::Enum(_) => true,
                TypeKind::Codec(codec) => admits_only_strings(cx, symbols, &codec.output, next),
                TypeKind::Union(union) => union
                    .variants
                    .iter()
                    .all(|variant| admits_only_strings(cx, symbols, &variant.ty, next)),
                TypeKind::Object(_) | TypeKind::Map(_) => false,
            }
        }
        TypeExpr::Optional(inner) => admits_only_strings(cx, symbols, inner, next),
        TypeExpr::Union(members) => members
            .iter()
            .all(|member| admits_only_strings(cx, symbols, member, next)),
        // Complete checking monomorphizes every instantiation first.
        TypeExpr::Instantiation(..) => true,
        TypeExpr::Nullable(_)
        | TypeExpr::List(_)
        | TypeExpr::Map(..)
        | TypeExpr::OrderedMap(..)
        | TypeExpr::Ref(_) => false,
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

/// A set's layer against its slot's duplicate policy: every set on a
/// layered slot names one of the slot's layers, and a set on any other
/// slot names none. An undeclared slot is reported by `slot_ref`.
fn set_layer(cx: &mut Cx<'_>, label: &str, site: Site, slot: Sym, layer: Option<Sym>) {
    let Some(decl) = cx.slots.iter().find(|s| s.name == slot) else {
        return;
    };
    let layered = decl.duplicates == SlotDuplicates::Layered;
    let declared = layer.is_none_or(|layer| decl.layers.contains(&layer));
    let spelled = cx.names.resolve(slot).to_owned();
    match layer {
        None if layered => {
            let reason = format!("set {spelled} names no layer, but slot {spelled} is layered");
            cx.malformed(site, label, &reason);
        }
        Some(layer) if !layered => {
            let layer = cx.names.resolve(layer).to_owned();
            let reason =
                format!("set {spelled} names layer {layer}, but slot {spelled} is not layered");
            cx.malformed(site, label, &reason);
        }
        Some(layer) if !declared => {
            let spelled = format!("{} of slot {spelled}", cx.names.resolve(layer));
            unresolved(cx, site, "context layer", &spelled);
        }
        _ => {}
    }
}

fn slot_ref(cx: &mut Cx<'_>, site: Site, slot: Sym) {
    if !cx.slots.iter().any(|s| s.name == slot) {
        let spelled = cx.names.resolve(slot).to_owned();
        unresolved(cx, site, "context slot", &spelled);
    }
}

/// A dependent value's `against`: the target field exists and holds a
/// reference to a type declaring the member field. Either shape broken
/// is malformed: the binder could only drop the payload silently.
fn dependent(
    cx: &mut Cx<'_>,
    node: &NodeDecl,
    label: &str,
    value: &crate::bindings::DependentBinding,
) {
    let Some(binding) = node.fields.iter().find(|f| f.name == value.target) else {
        let spelled = format!("{} of node {label}", cx.names.resolve(value.target));
        unresolved(cx, value.site, "field", &spelled);
        return;
    };
    let construct = format!("value binding of node {label}");
    let target = match crate::plan::field_type(&cx.types, node, binding) {
        Some(TypeExpr::Ref(inner)) => match inner.as_ref() {
            TypeExpr::Named(target) => Some(*target),
            _ => None,
        },
        _ => None,
    };
    let Some(target) = target else {
        let field = cx.names.resolve(value.target).to_owned();
        let reason = format!("the target field {field} is not a reference type");
        cx.malformed(value.site, &construct, &reason);
        return;
    };
    let Some(decl) = cx.types.lookup(target).and_then(|id| cx.types.get(id)) else {
        let spelled = cx.names.resolve(target).to_owned();
        unresolved(cx, value.site, "type", &spelled);
        return;
    };
    let declares = matches!(
        &decl.kind,
        TypeKind::Object(object) if object.fields.iter().any(|f| f.name == value.member)
    );
    if !declares {
        let member = cx.names.resolve(value.member).to_owned();
        let ty = cx.names.resolve(target).to_owned();
        let reason = format!("the member field {member} is not declared by type {ty}");
        cx.malformed(value.site, &construct, &reason);
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
