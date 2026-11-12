//! Demand-driven reading: which nodes a reading that wants only some
//! record types leaves unbound.
//!
//! A reading with a demand binds a node only when something the demand
//! names can come from it. Every other item-yielding node is skipped
//! where it opens: its text is scanned for its end, nothing in it is
//! decoded or validated, and it yields no item. What a node leaves for
//! the nodes after it never depends on a skipped one: a node whose
//! effects reach its siblings, an import, a node a descendant rule
//! counts, and every node on the way down to one of these or to a
//! demanded one stay bound.

use std::collections::HashSet;

use crate::bindings::{ChildPlan, NodeDecl, NodeDeclId, Role, ScopeId};
use crate::definition::Definition;
use crate::names::Sym;
use crate::types::{TypeExpr, TypeKind};

/// For each node declaration of `def`, whether a reading demanding the
/// record types `demand` names skips the nodes it declares. `Err` names
/// a demanded type the definition declares no record type for.
pub(crate) fn skips(def: &Definition, demand: &[String]) -> Result<Vec<bool>, String> {
    let mut wanted: HashSet<Sym> = HashSet::new();
    for name in demand {
        let declared = def
            .names
            .get(name)
            .filter(|sym| is_object(def, *sym))
            .ok_or_else(|| name.clone())?;
        wanted.insert(declared);
    }
    // A descendant rule counts the nodes of one name anywhere below its
    // owner, so a node of a counted name is never passed over, nor is
    // anything it is written inside.
    let counted: HashSet<Sym> = def
        .bindings
        .iter()
        .flat_map(|(_, decl)| decl.descendants.iter().map(|rule| rule.name))
        .collect();
    // A node is live when its own item is demanded, when it is an import
    // or leaves effects, when a rule counts it wherever it is written,
    // or when a live node can be written inside it.
    let mut live: Vec<bool> = def
        .bindings
        .iter()
        .map(|(id, decl)| {
            decl.role == Some(Role::SourceImport)
                || counted.contains(&decl.name)
                || binds_as(decl).any(|ty| wanted.contains(&ty))
                || def.plan.node(id).is_some_and(|plan| {
                    !plan.children_effects.steps.is_empty()
                        || !plan.following_effects.steps.is_empty()
                })
        })
        .collect();
    loop {
        let mut grown = false;
        for (id, _) in def.bindings.iter() {
            if live[id.index()] {
                continue;
            }
            let mut inside = Vec::new();
            if let Some(scope) = def.bindings.decl_scope(id) {
                declared_in(def, scope, &mut inside, &mut Vec::new());
            }
            if inside.iter().any(|child| live[child.index()]) {
                live[id.index()] = true;
                grown = true;
            }
        }
        if !grown {
            break;
        }
    }
    Ok(def
        .bindings
        .iter()
        .map(|(id, decl)| yields_item(decl) && !live[id.index()])
        .collect())
}

/// Whether a name declares an object type: records only ever bind to
/// object types.
fn is_object(def: &Definition, name: Sym) -> bool {
    def.types
        .lookup(name)
        .and_then(|id| def.types.get(id))
        .is_some_and(|decl| matches!(decl.kind, TypeKind::Object(_)))
}

/// Whether a node of this declaration yields an item of its own.
fn yields_item(decl: &NodeDecl) -> bool {
    decl.role.is_some_and(|role| role != Role::ContextControl)
}

/// The types a node of this declaration may bind as: its own, and each
/// its case dispatch may select.
fn binds_as(decl: &NodeDecl) -> impl Iterator<Item = Sym> + '_ {
    let selected = decl
        .cases
        .iter()
        .map(|case| &case.ty)
        .chain(&decl.otherwise)
        .filter_map(|ty| match ty {
            TypeExpr::Named(name) => Some(*name),
            _ => None,
        });
    decl.ty.into_iter().chain(selected)
}

/// Every node declaration a scope dispatches to, through the nested
/// scopes of the fields that consume undeclared children.
fn declared_in(
    def: &Definition,
    scope: ScopeId,
    found: &mut Vec<NodeDeclId>,
    seen: &mut Vec<ScopeId>,
) {
    if seen.contains(&scope) {
        return;
    }
    seen.push(scope);
    let Some(held) = def.bindings.scope(scope) else {
        return;
    };
    let plans = held
        .plans
        .values()
        .chain(held.case_plans.values().flat_map(|cases| cases.values()));
    for plan in plans {
        match plan {
            ChildPlan::Declared { decl, .. } => found.push(*decl),
            ChildPlan::Nested(inner) => declared_in(def, *inner, found, seen),
            ChildPlan::Scalar => {}
        }
    }
}
