//! Whole-child field sources: a `child`, `children`, or `entries`
//! source hands the field the feeding node's bound value, decoded by
//! the generated reader as the field's declared type — checked here,
//! at definition compile.

use crate::bindings::{BindingCatalog, FieldBinding, NodeDecl, Source};
use crate::compile::Cx;
use crate::diag::{INVALID_FIELD_CHILD_BINDING, Site};
use crate::names::Sym;
use crate::types::{TypeExpr, TypeKind};

/// One rejected field source.
struct Defect {
    site: Site,
    node: String,
    field: String,
    kind: &'static str,
    child: String,
    expected: String,
    types: String,
}

/// How many values a source hands its field, deciding which part of the
/// field's type one supplied value decodes as, and whether a node
/// holding a processing role can feed it.
#[derive(Clone, Copy)]
enum Collects {
    Single,
    Append,
    Keyed,
}

impl Collects {
    /// Whether one named node must be roleless to feed the field: a
    /// node holding a role emits as its own item and never reaches the
    /// parent's field.
    fn demands_roleless(self) -> bool {
        // A repeated `children` source is exempt: naming a role'd node
        // there admits it under the parent while its items stream
        // separately.
        !matches!(self, Collects::Append)
    }
}

pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    let mut found = Vec::new();
    for (_, owner) in bindings.iter() {
        for field in &owner.fields {
            for source in &field.sources {
                let (kind, child, collects) = match source {
                    // Nested sources select parts of the child, not its
                    // whole value; the field holds those parts, typed
                    // on their own.
                    Source::Child { name, sources, .. } if sources.is_empty() => {
                        ("child", *name, Collects::Single)
                    }
                    Source::Children { name, sources, .. } if sources.is_empty() => {
                        ("children", *name, Collects::Append)
                    }
                    Source::Entries { node, .. } => ("entries", *node, Collects::Keyed),
                    _ => continue,
                };
                let Some(feeding) = child_decl(bindings, owner, child) else {
                    // No declaration is placed under the owner, so the
                    // binder hands the field the child's one positional
                    // value — the same scalars for a single `child` and
                    // a repeated `children` source. An entries source
                    // is refused outright in the scope pass.
                    if !matches!(collects, Collects::Single | Collects::Append) {
                        continue;
                    }
                    let Some(TypeExpr::Named(name)) = expected(cx, owner, field, collects) else {
                        continue;
                    };
                    if !matches!(
                        cx.types
                            .lookup(name)
                            .and_then(|id| cx.types.get(id))
                            .map(|decl| &decl.kind),
                        Some(TypeKind::Object(_)) | Some(TypeKind::Union(_))
                    ) {
                        continue;
                    }
                    found.push(Defect {
                        site: field.site,
                        node: cx.names.resolve(owner.name).to_owned(),
                        field: cx.names.resolve(field.name).to_owned(),
                        kind,
                        child: cx.names.resolve(child).to_owned(),
                        expected: cx.names.resolve(name).to_owned(),
                        types: "a scalar, as no node of this name is declared here".to_owned(),
                    });
                    continue;
                };
                let expected = expected(cx, owner, field, collects);
                if let Some(role) = feeding.role.filter(|_| collects.demands_roleless()) {
                    found.push(Defect {
                        site: field.site,
                        node: cx.names.resolve(owner.name).to_owned(),
                        field: cx.names.resolve(field.name).to_owned(),
                        kind,
                        child: cx.names.resolve(child).to_owned(),
                        expected: expected
                            .as_ref()
                            .map_or_else(|| "-".to_owned(), |ty| spelling(cx, ty)),
                        types: format!("role {}, an item of its own", role.name()),
                    });
                    continue;
                }
                if let Some(ty @ TypeExpr::Union(_)) = &expected
                    && !feeding.cases.is_empty()
                {
                    // An inline union decodes untagged, so the objects a
                    // case dispatch selects cannot be told apart; only a
                    // named union's discriminators keep them distinct.
                    found.push(Defect {
                        site: field.site,
                        node: cx.names.resolve(owner.name).to_owned(),
                        field: cx.names.resolve(field.name).to_owned(),
                        kind,
                        child: cx.names.resolve(child).to_owned(),
                        expected: spelling(cx, ty),
                        types: item_types(cx, feeding).join(" or "),
                    });
                    continue;
                }
                let Some(TypeExpr::Named(name)) = expected else {
                    continue;
                };
                if conforms(cx, name, feeding) {
                    continue;
                }
                found.push(Defect {
                    site: field.site,
                    node: cx.names.resolve(owner.name).to_owned(),
                    field: cx.names.resolve(field.name).to_owned(),
                    kind,
                    child: cx.names.resolve(child).to_owned(),
                    expected: spelling(cx, &TypeExpr::Named(name)),
                    types: item_types(cx, feeding).join(" or "),
                });
            }
        }
    }
    for defect in found {
        let Defect {
            site,
            node,
            field,
            kind,
            child,
            expected,
            types,
        } = defect;
        cx.diags.error(
            INVALID_FIELD_CHILD_BINDING,
            site,
            &[&node, &field, kind, &child, &expected, &types],
        );
    }
}

/// The declaration of the child named by a source of `owner`'s field,
/// placed under `owner`. A source naming no declaration hands the
/// binder scalars, and its type is not ours to check.
fn child_decl<'b>(
    bindings: &'b BindingCatalog,
    owner: &NodeDecl,
    child: Sym,
) -> Option<&'b NodeDecl> {
    bindings
        .iter()
        .map(|(_, decl)| decl)
        .find(|decl| decl.name == child && decl.placement.parents().contains(&owner.name))
}

/// Whether a value the child binds decodes as the named type: the name
/// is one the child's items carry or a union spells — the variants a
/// case dispatch selects, or the variant whose discriminator is the
/// child's own name.
fn conforms(cx: &Cx<'_>, name: Sym, child: &NodeDecl) -> bool {
    let union = cx
        .types
        .lookup(name)
        .and_then(|id| cx.types.get(id))
        .and_then(|decl| match &decl.kind {
            TypeKind::Union(union) => Some(union),
            _ => None,
        });
    let Some(union) = union else {
        return item_types(cx, child).contains(&cx.names.resolve(name).to_owned());
    };
    if child.cases.is_empty() {
        // The node's own name spells the variant, and its type is the
        // variant's.
        return child.ty.is_some_and(|own| {
            union.variants.iter().any(|variant| {
                variant.discriminator == child.name
                    && matches!(variant.ty, TypeExpr::Named(ty) if ty == own)
            })
        });
    }
    // A case dispatch selects a type the union must hold as a variant.
    let variants: Vec<Sym> = union
        .variants
        .iter()
        .filter_map(|variant| match variant.ty {
            TypeExpr::Named(ty) => Some(ty),
            _ => None,
        })
        .collect();
    item_types(cx, child).iter().all(|selected| {
        variants
            .iter()
            .any(|variant| cx.names.resolve(*variant) == selected)
    })
}

/// The type names a child's items carry, as binding emits them: the
/// type a case or `otherwise` selects, else the node's own.
fn item_types(cx: &Cx<'_>, child: &NodeDecl) -> Vec<String> {
    let own = cx.names.resolve(child.ty.unwrap_or(child.name)).to_owned();
    if child.cases.is_empty() {
        return vec![own];
    }
    let selected = child.cases.iter().map(|case| &case.ty);
    let mut types = Vec::new();
    for ty in selected.chain(&child.otherwise) {
        let name = match ty {
            TypeExpr::Named(name) => cx.names.resolve(*name).to_owned(),
            _ => own.clone(),
        };
        if !types.contains(&name) {
            types.push(name);
        }
    }
    types
}

/// The type one supplied value decodes as, mirroring the binder: the
/// field's declared type, or its element type when the field collects.
fn expected(
    cx: &Cx<'_>,
    owner: &NodeDecl,
    field: &FieldBinding,
    collects: Collects,
) -> Option<TypeExpr> {
    let mut ty = field_type(cx, owner, field)?;
    while let TypeExpr::Optional(inner) = ty {
        ty = *inner;
    }
    // A declared map type carries its key and value apart from the
    // expression.
    let ty = match collects {
        Collects::Append => match ty {
            TypeExpr::List(inner) => *inner,
            ty => ty,
        },
        Collects::Keyed => match ty {
            TypeExpr::Map(_, value) | TypeExpr::OrderedMap(_, value) => *value,
            TypeExpr::Named(name) => cx
                .types
                .lookup(name)
                .and_then(|id| cx.types.get(id))
                .and_then(|decl| match &decl.kind {
                    TypeKind::Map(map) => Some(map.value.clone()),
                    _ => None,
                })
                .unwrap_or(TypeExpr::Named(name)),
            ty => ty,
        },
        Collects::Single => ty,
    };
    Some(ty)
}

/// The declared type of a node's field: from its `type=` object when it
/// has one, else from the inline binding.
fn field_type(cx: &Cx<'_>, node: &NodeDecl, field: &FieldBinding) -> Option<TypeExpr> {
    if let Some(ty) = &field.ty {
        return Some(ty.clone());
    }
    let decl = cx.types.get(cx.types.lookup(node.ty?)?)?;
    match &decl.kind {
        TypeKind::Object(object) => object
            .fields
            .iter()
            .find(|held| held.name == field.name)
            .map(|held| held.ty.clone()),
        _ => None,
    }
}

fn spelling(cx: &Cx<'_>, ty: &TypeExpr) -> String {
    crate::summary::expr(&cx.names, ty)
}
