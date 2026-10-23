//! When branches of children rules: the disjointness proof applied when
//! a rule is collected, and the checks needing the schema's types (a
//! scalar or enum gating field, a decodable `equals`).

use kdlith_model::{Location, Position, SourceId, Span, Value};

use crate::bindings::{BindingCatalog, Condition, NodeDecl, WhenBranch};
use crate::compile::Cx;
use crate::compile::link::unresolved;
use crate::decode::{self, Decoder};
use crate::diag::{INVALID_CONDITION_PROPERTY, Site};
use crate::names::Sym;
use crate::summary;
use crate::types::{Builtin, TypeExpr, TypeKind};

/// Whether every pair of branches is provably disjoint: some property
/// carries a required leaf in both with structurally distinct values, so
/// no node instance can match both. Unprovable pairs are refused.
pub(crate) fn disjoint(branches: &[WhenBranch]) -> bool {
    let required: Vec<Vec<(Sym, &Value)>> = branches
        .iter()
        .map(|branch| {
            let mut leaves = Vec::new();
            required_leaves(&branch.condition, &mut leaves);
            leaves
        })
        .collect();
    (0..required.len()).all(|a| {
        (a + 1..required.len()).all(|b| {
            required[a].iter().any(|(property, value)| {
                required[b]
                    .iter()
                    .any(|(other, held)| other == property && held != value)
            })
        })
    })
}

/// The leaves every match of `condition` satisfies: those reached
/// through conjunctions only.
fn required_leaves<'c>(condition: &'c Condition, out: &mut Vec<(Sym, &'c Value)>) {
    match condition {
        Condition::Equals { property, equals } => out.push((*property, equals)),
        Condition::All(members) => {
            for member in members {
                required_leaves(member, out);
            }
        }
        Condition::Any(_) => {}
    }
}

/// Check every condition leaf against its node's declared fields, after
/// linking has resolved the type names. Each defect is reported once per
/// written rule, however many rules a group rule expanded to.
pub(crate) fn run(cx: &mut Cx<'_>, bindings: &BindingCatalog) {
    let mut found: Vec<(Site, Defect)> = Vec::new();
    // Condition values are literals of the definition itself; no source
    // location is carried into a value decoded from one.
    let nowhere = Location::new(
        SourceId(0),
        Span::new(Position::new(0, 1, 1), Position::new(0, 1, 1)),
    );
    let (decoder, _) = Decoder::new(&cx.types);
    let mut seen: Vec<(Site, Sym, Value)> = Vec::new();
    for (_, node) in bindings.iter() {
        let label = cx.names.resolve(node.name).to_owned();
        for rule in &node.children {
            let mut leaves = Vec::new();
            for branch in &rule.branches {
                every_leaf(&branch.condition, &mut leaves);
            }
            for (property, equals) in leaves {
                let key = (rule.site, property, equals.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                let spelled = cx.names.resolve(property).to_owned();
                let Some(ty) = field_type(cx, node, property) else {
                    let name = format!("{spelled} of node {label}");
                    found.push((rule.site, Defect::Undeclared(name)));
                    continue;
                };
                if !gateable(cx, &ty) {
                    let shown = summary::expr(&cx.names, &ty);
                    found.push((
                        rule.site,
                        Defect::NotGateable([label.clone(), spelled, shown]),
                    ));
                    continue;
                }
                if decoder
                    .decode(&cx.types, &cx.names, &ty, equals, nowhere)
                    .is_none()
                {
                    let reason = format!(
                        "when property {spelled} equals {}, which does not decode as {}",
                        decode::found(equals),
                        summary::expr(&cx.names, &ty)
                    );
                    found.push((
                        rule.site,
                        Defect::Undecodable(format!("node {label}"), reason),
                    ));
                }
            }
        }
    }
    for (site, defect) in found {
        match defect {
            Defect::Undeclared(name) => unresolved(cx, site, "field", &name),
            Defect::NotGateable([node, field, ty]) => {
                cx.diags
                    .error(INVALID_CONDITION_PROPERTY, site, &[&node, &field, &ty]);
            }
            Defect::Undecodable(label, reason) => cx.malformed(site, &label, &reason),
        }
    }
}

/// One rejected condition leaf.
enum Defect {
    /// The node declares no field of that name.
    Undeclared(String),
    /// Node, field, and the field's type, which no condition gates on.
    NotGateable([String; 3]),
    /// The `equals` value does not decode as the field's type.
    Undecodable(String, String),
}

fn every_leaf<'c>(condition: &'c Condition, out: &mut Vec<(Sym, &'c Value)>) {
    match condition {
        Condition::Equals { property, equals } => out.push((*property, equals)),
        Condition::All(members) | Condition::Any(members) => {
            for member in members {
                every_leaf(member, out);
            }
        }
    }
}

/// The declared type of the node's field named `property`: from its
/// `type=` object when it has one, else from the inline binding.
fn field_type(cx: &Cx<'_>, node: &NodeDecl, property: Sym) -> Option<TypeExpr> {
    let binding = node.fields.iter().find(|field| field.name == property)?;
    if let Some(ty) = &binding.ty {
        return Some(ty.clone());
    }
    let decl = cx.types.get(cx.types.lookup(node.ty?)?)?;
    match &decl.kind {
        TypeKind::Object(object) => object
            .fields
            .iter()
            .find(|field| field.name == property)
            .map(|field| field.ty.clone()),
        _ => None,
    }
}

/// Whether a condition may gate on a field of type `ty`: one scalar or
/// enum value, possibly optional or nullable. References, codecs,
/// unions, and composite types hold nothing scalar equality is defined
/// over.
fn gateable(cx: &Cx<'_>, ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => gateable(cx, inner),
        TypeExpr::Named(name) => {
            let kind = cx
                .types
                .lookup(*name)
                .and_then(|id| cx.types.get(id))
                .map(|decl| &decl.kind);
            matches!(
                kind,
                Some(
                    TypeKind::Builtin(
                        Builtin::String
                            | Builtin::Bool
                            | Builtin::Integer
                            | Builtin::Decimal
                            | Builtin::U64
                            | Builtin::ValueExpr
                    ) | TypeKind::String(_)
                        | TypeKind::Enum(_)
                )
            )
        }
        _ => false,
    }
}
