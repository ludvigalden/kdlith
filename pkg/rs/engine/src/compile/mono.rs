//! Monomorphization: each instantiation `T<A, B>` becomes the
//! declaration's body with arguments substituted, so no generic
//! survives into a compiled plan. A cycle is `KDLITH-SCH009`.

use std::collections::BTreeMap;

use crate::compile::Cx;
use crate::diag::{INSTANTIATION_CYCLE, Site};
use crate::names::Sym;
use crate::summary;
use crate::types::{TypeDecl, TypeExpr, TypeKind};

/// A definition expands at most this many instantiations; deeper need
/// is indistinguishable from a runaway graph.
const MAX_INSTANTIATIONS: usize = 256;

/// The state carried across one monomorphization pass.
struct Pass {
    /// Rendered instantiation to the declaration name it became.
    done: BTreeMap<String, Sym>,
    /// The generic heads currently being expanded; a head reappearing
    /// inside its own expansion is a cycle.
    open: Vec<Sym>,
    budget: usize,
}

pub(crate) fn run(cx: &mut Cx<'_>) {
    let mut pass = Pass {
        done: BTreeMap::new(),
        open: Vec::new(),
        budget: MAX_INSTANTIATIONS,
    };
    let ids: Vec<crate::types::TypeId> = cx.types.iter().map(|(id, _)| id).collect();
    for id in ids {
        let Some(site) = cx.types.get(id).and_then(|decl| decl.site) else {
            continue;
        };
        let Some(mut decl) = cx.types.take_decl(id) else {
            continue;
        };
        pass.open.push(decl.name);
        walk_kind(cx, &mut pass, site, &mut decl.kind);
        pass.open.pop();
        cx.types.put_decl(id, decl);
    }
    let mut nodes = std::mem::take(&mut cx.nodes);
    for node in &mut nodes {
        for field in &mut node.fields {
            if let Some(ty) = &mut field.ty {
                pass.expr(cx, field.site, ty);
            }
        }
        for case in &mut node.cases {
            pass.expr(cx, case.site, &mut case.ty);
        }
        if let Some(ty) = &mut node.otherwise {
            pass.expr(cx, node.site, ty);
        }
    }
    cx.nodes = nodes;
    let mut slots = std::mem::take(&mut cx.slots);
    for slot in &mut slots {
        pass.expr(cx, slot.site, &mut slot.ty);
    }
    cx.slots = slots;
}

/// Rewrite every type expression one declared kind holds.
fn walk_kind(cx: &mut Cx<'_>, pass: &mut Pass, site: Site, kind: &mut TypeKind) {
    match kind {
        TypeKind::Builtin(_) | TypeKind::String(_) | TypeKind::Enum(_) | TypeKind::Asset(_) => {}
        TypeKind::Object(object) => {
            for field in &mut object.fields {
                pass.expr(cx, site, &mut field.ty);
            }
        }
        TypeKind::Union(union) => {
            for variant in &mut union.variants {
                pass.expr(cx, site, &mut variant.ty);
            }
        }
        TypeKind::Map(map) => {
            pass.expr(cx, site, &mut map.key);
            pass.expr(cx, site, &mut map.value);
        }
        TypeKind::Codec(codec) => {
            pass.expr(cx, site, &mut codec.output);
            for capture in &mut codec.captures {
                pass.expr(cx, site, &mut capture.ty);
            }
        }
    }
}

impl Pass {
    /// Rewrite one type expression, expanding the instantiation it may
    /// name into the declaration it monomorphizes to.
    fn expr(&mut self, cx: &mut Cx<'_>, site: Site, ty: &mut TypeExpr) {
        match ty {
            TypeExpr::Named(name) => {
                if self.open.contains(name) {
                    // The declaration's own head, seen while it is taken
                    // out for walking; an instantiation of it reports the
                    // cycle at its own arm.
                    return;
                }
                if let Some(decl) = cx.types.lookup(*name).and_then(|id| cx.types.get(id)) {
                    if !decl.parameters.is_empty() {
                        let spelled = cx.names.resolve(*name).to_owned();
                        cx.malformed(
                            site,
                            &format!("type {spelled}"),
                            "is generic and needs type arguments",
                        );
                    }
                }
            }
            TypeExpr::Instantiation(head, args) => {
                for arg in &mut *args {
                    self.expr(cx, site, arg);
                }
                // Snapshot everything borrowed from `*ty` before the
                // assignment below ends the borrows.
                let head = *head;
                let args = args.clone();
                let spelled = cx.names.resolve(head).to_owned();
                let rendered = format!(
                    "{spelled}<{}>",
                    args.iter()
                        .map(|a| summary::expr(&cx.names, a))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                if let Some(sym) = self.done.get(&rendered) {
                    *ty = TypeExpr::Named(*sym);
                    return;
                }
                if self.open.contains(&head) {
                    let mut cycle: Vec<String> = self
                        .open
                        .iter()
                        .map(|open| cx.names.resolve(*open).to_owned())
                        .collect();
                    cycle.push(spelled);
                    let cycle = cycle.join(" -> ");
                    cx.diags.error(INSTANTIATION_CYCLE, site, &[&cycle]);
                    return;
                }
                if self.budget == 0 {
                    cx.malformed(
                        site,
                        &format!("instantiation {rendered}"),
                        &format!(
                            "a definition expands more than {MAX_INSTANTIATIONS} instantiations"
                        ),
                    );
                    return;
                }
                let Some(decl) = cx.types.lookup(head).and_then(|id| cx.types.get(id)) else {
                    // Linking reports the unresolved head.
                    return;
                };
                let params = decl.parameters.clone();
                if params.is_empty() {
                    cx.malformed(
                        site,
                        &format!("type {spelled}"),
                        "is not generic but is instantiated with type arguments",
                    );
                    return;
                }
                if params.len() != args.len() {
                    cx.malformed(
                        site,
                        &format!("type {spelled}"),
                        &format!(
                            "takes {} type arguments, found {}",
                            params.len(),
                            args.len()
                        ),
                    );
                    return;
                }
                let substituted = substitute_kind(&decl.kind, &params, &args);
                self.budget -= 1;
                let sym = cx.names.intern(&rendered);
                let fresh = TypeDecl {
                    name: sym,
                    parameters: Vec::new(),
                    kind: substituted,
                    site: Some(site),
                };
                let pushed = cx.types.push(fresh);
                *ty = TypeExpr::Named(sym);
                self.done.insert(rendered, sym);
                if let Ok(id) = pushed
                    && let Some(mut decl) = cx.types.take_decl(id)
                {
                    self.open.push(head);
                    walk_kind(cx, self, site, &mut decl.kind);
                    self.open.pop();
                    cx.types.put_decl(id, decl);
                }
            }
            TypeExpr::Optional(inner)
            | TypeExpr::Nullable(inner)
            | TypeExpr::List(inner)
            | TypeExpr::Ref(inner) => self.expr(cx, site, inner),
            TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
                self.expr(cx, site, key);
                self.expr(cx, site, value);
            }
            TypeExpr::Union(members) => {
                for member in members {
                    self.expr(cx, site, member);
                }
            }
        }
    }
}

/// A declaration's body with arguments substituted for parameters. The
/// substituted body may still name instantiations; the pass expands
/// those when it walks the fresh declaration.
fn substitute_kind(kind: &TypeKind, params: &[Sym], args: &[TypeExpr]) -> TypeKind {
    let mut cloned = kind.clone();
    match &mut cloned {
        TypeKind::Builtin(_) | TypeKind::String(_) | TypeKind::Enum(_) | TypeKind::Asset(_) => {}
        TypeKind::Object(object) => {
            for field in &mut object.fields {
                substitute_expr(&mut field.ty, params, args);
            }
        }
        TypeKind::Union(union) => {
            for variant in &mut union.variants {
                substitute_expr(&mut variant.ty, params, args);
            }
        }
        TypeKind::Map(map) => {
            substitute_expr(&mut map.key, params, args);
            substitute_expr(&mut map.value, params, args);
        }
        TypeKind::Codec(codec) => {
            substitute_expr(&mut codec.output, params, args);
            for capture in &mut codec.captures {
                substitute_expr(&mut capture.ty, params, args);
            }
        }
    }
    cloned
}

fn substitute_expr(ty: &mut TypeExpr, params: &[Sym], args: &[TypeExpr]) {
    match ty {
        TypeExpr::Named(name) => {
            if let Some(at) = params.iter().position(|param| param == name) {
                *ty = args[at].clone();
            }
        }
        TypeExpr::Instantiation(_, inner) => {
            for arg in inner {
                substitute_expr(arg, params, args);
            }
        }
        TypeExpr::Optional(inner)
        | TypeExpr::Nullable(inner)
        | TypeExpr::List(inner)
        | TypeExpr::Ref(inner) => substitute_expr(inner, params, args),
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            substitute_expr(key, params, args);
            substitute_expr(value, params, args);
        }
        TypeExpr::Union(members) => {
            for member in members {
                substitute_expr(member, params, args);
            }
        }
    }
}
