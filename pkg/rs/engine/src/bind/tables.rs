//! Binding tables, built once per definition from its compiled catalogs:
//! per node declaration, the logical fields a bound node produces; per
//! scope, the field each argument, property, and child supplies.

use std::collections::BTreeMap;

use kdlith_model::Value;
use kdlith_runtime::SlotId;

use crate::bindings::{
    CaseRule, ContextPolicy, DependentBinding, NodeDecl, NodeDeclId, Role, ScopeId, Source,
};
use crate::definition::Definition;
use crate::names::Sym;
use crate::plan::DeferredBinding;
use crate::types::{Sensitivity, TypeExpr, TypeKind};

/// How a supply lands in its field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The field holds one value; a second supply collides.
    Single,
    /// The field collects values into a list, in document order.
    Append,
    /// The field collects keyed entries; a repeated key is rejected.
    Keyed,
}

/// The field of the owning declaration a piece of syntax supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bind {
    pub(crate) field: usize,
    pub(crate) mode: Mode,
}

/// A field read from a context slot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContextBind {
    pub(crate) slot: SlotId,
    pub(crate) name: Sym,
    pub(crate) policy: Option<ContextPolicy>,
}

/// What an unsupplied, non-optional collection field defaults to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Collect {
    No,
    List,
    Map,
}

/// One logical field of a declaration's bound value.
#[derive(Clone, Debug)]
pub(crate) struct FieldInfo {
    pub(crate) name: Sym,
    pub(crate) ty: Option<TypeExpr>,
    pub(crate) required: bool,
    pub(crate) collect: Collect,
    pub(crate) fallback: Option<Value>,
    pub(crate) context: Option<ContextBind>,
    /// The sensitivity the field's declaration tags it with, enforced
    /// by the output policy.
    pub(crate) sensitivity: Option<Sensitivity>,
    /// The declaration-group expansions splicing into this field's
    /// collection, as (target, member) pairs.
    pub(crate) expand: Vec<(Sym, Sym)>,
}

/// Everything the binder needs about one node declaration.
#[derive(Clone, Debug)]
pub(crate) struct DeclInfo {
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) type_name: String,
    /// The field carrying a declaration's identity.
    pub(crate) identity: Option<usize>,
    /// The fields of an import's path (argument 0), its projected slots
    /// (`project`), and its repeat policy (`repeated`).
    pub(crate) path: Option<usize>,
    pub(crate) project: Option<usize>,
    pub(crate) repeated: Option<usize>,
    /// Fields holding `ref<T>` values, which keep the item provisional.
    pub(crate) references: Vec<usize>,
    pub(crate) provisional: bool,
    /// The case dispatch selecting the type a node binds as.
    pub(crate) cases: Vec<CaseRule>,
    pub(crate) otherwise: Option<TypeExpr>,
    /// The dependent payload binding, when the node declares one.
    pub(crate) dependent: Option<DependentBinding>,
}

/// The syntax of one scope's owner: which owner field each argument,
/// property, and whole or keyed child supplies.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub(crate) args: Vec<Option<Bind>>,
    pub(crate) rest: Option<Bind>,
    pub(crate) props: Vec<(Sym, Bind)>,
    pub(crate) children: Vec<(Sym, Bind)>,
}

impl Table {
    pub(crate) fn arg(&self, at: u64) -> Option<Bind> {
        let at = usize::try_from(at).ok()?;
        self.args.get(at).copied().flatten().or(self.rest)
    }

    pub(crate) fn prop(&self, name: Sym) -> Option<Bind> {
        self.props.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }

    pub(crate) fn child(&self, name: Sym) -> Option<Bind> {
        self.children
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| *b)
    }
}

/// The tables of one definition: decl infos by declaration, tables by
/// scope (the root scope has none; its nodes supply nothing).
pub(crate) struct Tables {
    pub(crate) decls: Vec<DeclInfo>,
    pub(crate) scopes: Vec<Option<Table>>,
    /// Each slot's name and type, by slot index.
    pub(crate) slots: Vec<(Sym, TypeExpr)>,
}

/// One binding source and the owner field it supplies.
type Use<'d> = (usize, &'d Source);

pub(crate) fn build(def: &Definition) -> Tables {
    let mut slots: Vec<(SlotId, Sym, TypeExpr)> = def
        .slots
        .iter()
        .filter_map(|slot| {
            Some((
                def.plan.context.slot(slot.name)?,
                slot.name,
                slot.ty.clone(),
            ))
        })
        .collect();
    slots.sort_by_key(|(id, ..)| *id);
    let mut tables = Tables {
        decls: Vec::with_capacity(def.bindings.len()),
        scopes: vec![None; def.bindings.scopes().len()],
        slots: slots.into_iter().map(|(_, name, ty)| (name, ty)).collect(),
    };
    for (id, decl) in def.bindings.iter() {
        let info = decl_info(def, id, decl);
        let mut uses: Vec<Use<'_>> = Vec::new();
        for binding in &decl.fields {
            let Some(at) = info.fields.iter().position(|f| f.name == binding.name) else {
                continue;
            };
            uses.extend(binding.sources.iter().map(|source| (at, source)));
        }
        tables.decls.push(info);
        if let Some(scope) = def.bindings.decl_scope(id) {
            fill(def, &mut tables.scopes, scope, &uses, false);
        }
    }
    tables
}

/// Build the table of one scope from the sources that select from it,
/// then the tables of the children its nested sources select from.
fn fill(
    def: &Definition,
    scopes: &mut [Option<Table>],
    scope: ScopeId,
    uses: &[Use<'_>],
    many: bool,
) {
    let single = if many { Mode::Append } else { Mode::Single };
    let mut table = Table {
        args: Vec::new(),
        rest: None,
        props: Vec::new(),
        children: Vec::new(),
    };
    let mut nested: BTreeMap<Sym, (bool, Vec<Use<'_>>)> = BTreeMap::new();
    for &(field, source) in uses {
        match source {
            Source::Argument(index) => {
                let Ok(at) = usize::try_from(*index) else {
                    continue;
                };
                if table.args.len() <= at {
                    table.args.resize(at + 1, None);
                }
                table.args[at] = Some(Bind {
                    field,
                    mode: single,
                });
            }
            Source::Args => {
                table.rest = Some(Bind {
                    field,
                    mode: Mode::Append,
                });
            }
            Source::Property(name) => table.props.push((
                *name,
                Bind {
                    field,
                    mode: single,
                },
            )),
            Source::Child { name, sources } | Source::Children { name, sources } => {
                let repeated = many || matches!(source, Source::Children { .. });
                if sources.is_empty() {
                    let mode = if repeated { Mode::Append } else { Mode::Single };
                    table.children.push((*name, Bind { field, mode }));
                } else {
                    let entry = nested.entry(*name).or_insert((repeated, Vec::new()));
                    entry.0 |= repeated;
                    entry.1.extend(sources.iter().map(|inner| (field, inner)));
                }
            }
            Source::Entries { node, .. } => table.children.push((
                *node,
                Bind {
                    field,
                    mode: Mode::Keyed,
                },
            )),
            // Expansion splices at field finalization, not per entry.
            Source::Expand { .. } | Source::Context { .. } => {}
        }
    }
    if let Some(at) = scopes.get_mut(scope.index()) {
        *at = Some(table);
    }
    for (name, (repeated, inner)) in nested {
        if let Some(crate::bindings::ChildPlan::Nested(child)) = def.bindings.child(scope, name) {
            fill(def, scopes, child, &inner, repeated);
        }
    }
}

fn decl_info(def: &Definition, id: NodeDeclId, decl: &NodeDecl) -> DeclInfo {
    let object = decl
        .ty
        .and_then(|ty| def.types.lookup(ty))
        .and_then(|ty| def.types.get(ty))
        .and_then(|ty| match &ty.kind {
            TypeKind::Object(object) => Some(object),
            _ => None,
        });
    let mut fields: Vec<FieldInfo> = match object {
        Some(object) => object
            .fields
            .iter()
            .map(|field| FieldInfo {
                name: field.name,
                ty: Some(field.ty.clone()),
                required: !optional(&field.ty),
                collect: Collect::No,
                fallback: field.fallback.clone(),
                context: None,
                sensitivity: field.sensitivity,
                expand: Vec::new(),
            })
            .collect(),
        None => decl
            .fields
            .iter()
            .map(|binding| FieldInfo {
                name: binding.name,
                ty: binding.ty.clone(),
                required: binding.ty.as_ref().is_some_and(|ty| !optional(ty)),
                collect: Collect::No,
                fallback: None,
                context: None,
                sensitivity: None,
                expand: Vec::new(),
            })
            .collect(),
    };
    let mut path = None;
    for binding in &decl.fields {
        let Some(at) = fields.iter().position(|f| f.name == binding.name) else {
            continue;
        };
        let info = &mut fields[at];
        if info.required {
            info.collect = collect(&binding.sources);
        }
        for source in &binding.sources {
            match source {
                Source::Context { slot, policy } => {
                    if let Some(id) = def.plan.context.slot(*slot) {
                        info.context = Some(ContextBind {
                            slot: id,
                            name: *slot,
                            policy: *policy,
                        });
                    }
                }
                Source::Expand { target, member } => {
                    info.expand.push((*target, *member));
                }
                Source::Argument(0) => path = Some(at),
                _ => {}
            }
        }
    }
    let position = |name: Sym| fields.iter().position(|f| f.name == name);
    let import = decl.role == Some(Role::SourceImport);
    let import_field = |name: &str| def.names.get(name).and_then(position).filter(|_| import);
    let identity = def
        .symbols
        .declaration(id)
        .and_then(|d| d.identity)
        .and_then(position);
    let deferred = def.plan.node(id).map_or(&[][..], |plan| &plan.deferred);
    let references = deferred
        .iter()
        .filter_map(|binding| match binding {
            DeferredBinding::Reference { field, .. } => position(*field),
            DeferredBinding::Contribution => None,
        })
        .collect();
    DeclInfo {
        type_name: def.names.resolve(decl.ty.unwrap_or(decl.name)).to_owned(),
        identity,
        path: path.filter(|_| import),
        project: import_field("project"),
        repeated: import_field("repeated"),
        references,
        provisional: !deferred.is_empty(),
        cases: decl.cases.clone(),
        otherwise: decl.otherwise.clone(),
        dependent: decl.value.clone(),
        fields,
    }
}

fn optional(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Optional(_))
}

/// Whether a field's sources collect, so that zero occurrences mean an
/// empty collection rather than an absent field.
fn collect(sources: &[Source]) -> Collect {
    let mut found = Collect::No;
    for source in sources {
        match source {
            Source::Entries { .. } => return Collect::Map,
            Source::Args | Source::Children { .. } => found = Collect::List,
            Source::Child { sources, .. } => {
                if collect(sources) != Collect::No {
                    found = Collect::List;
                }
            }
            _ => {}
        }
    }
    found
}
