//! Binding tables, built once per definition from its compiled catalogs:
//! per node declaration, the logical fields a bound node produces; per
//! scope, the field each argument, property, and child supplies.

use std::collections::BTreeMap;

use kdlith_model::Value;
use kdlith_runtime::SlotId;

use crate::bindings::{
    CaseRule, ContextPolicy, DependentBinding, NodeDecl, NodeDeclId, PathValueBinding, Role,
    ScopeId, Source,
};
use crate::definition::{Definition, ReferenceDecl};
use crate::names::Sym;
use crate::plan::DeferredBinding;
use crate::types::{DefaultSpec, Sensitivity, TypeExpr, TypeKind};

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
    /// The case whose match admits this field, when a case-level
    /// binding declares it: the field binds only under that case.
    pub(crate) case: Option<usize>,
    pub(crate) ty: Option<TypeExpr>,
    pub(crate) required: bool,
    pub(crate) collect: Collect,
    pub(crate) fallback: Option<Value>,
    /// A computed fallback's declared dependencies: the descriptor the
    /// registered value resolver receives when the field is absent.
    /// The engine never computes it.
    pub(crate) computed: Option<Vec<String>>,
    pub(crate) context: Option<ContextBind>,
    /// The sensitivity the field's declaration tags it with, enforced
    /// by the output policy.
    pub(crate) sensitivity: Option<Sensitivity>,
    /// The declaration-group expansions splicing into this field's
    /// collection, as (target, member) pairs.
    pub(crate) expand: Vec<(Sym, Sym)>,
    /// The declared reference type the field is typed by, whose codec
    /// parses its spelling.
    pub(crate) reference: Option<ReferenceDecl>,
    /// The context slot an unqualified reference the field holds takes
    /// its qualifier from, when the field binding or its reference type
    /// names one; `None` defers to the definition's default slot.
    pub(crate) qualify: Option<SlotId>,
}

/// Everything the binder needs about one node declaration.
#[derive(Clone, Debug)]
pub(crate) struct DeclInfo {
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) type_name: String,
    /// How a declaration's item composes its identity.
    pub(crate) identity: IdentityBind,
    /// The fields of an import's path (argument 0), its projected slots
    /// (`project`), its repeat policy (`repeated`), and its `as=`
    /// namespace (`namespace`).
    pub(crate) path: Option<usize>,
    pub(crate) project: Option<usize>,
    pub(crate) repeated: Option<usize>,
    pub(crate) namespace: Option<usize>,
    /// The fields of a contribution's layer and application scope, which
    /// the preserved record carries as its metadata.
    pub(crate) layer: Option<usize>,
    pub(crate) scope: Option<usize>,
    /// Fields holding `ref<T>` or declared reference type values, which
    /// keep the item provisional.
    pub(crate) references: Vec<usize>,
    pub(crate) provisional: bool,
    /// The case dispatch selecting the type a node binds as.
    pub(crate) cases: Vec<CaseRule>,
    pub(crate) otherwise: Option<TypeExpr>,
    /// The dependent payload binding, when the node declares one.
    pub(crate) dependent: Option<DependentBinding>,
    /// The path-typed payload binding, when the node declares one.
    pub(crate) path_value: Option<PathValueBinding>,
}

/// How a declaration composes its identity.
#[derive(Clone, Debug)]
pub(crate) enum IdentityBind {
    /// No identity block: the domain is the declaration's type and the
    /// name is argument 0's field, when one binds it.
    Fallback(Option<usize>),
    /// An identity block: its domain, and every field it composes, in
    /// order, with the field's position when the node declares it.
    Composed {
        domain: String,
        fields: Vec<(Sym, Option<usize>)>,
    },
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
    /// Each slot's name, type, declared accumulation operation, and
    /// declared duplicate policy, by slot index.
    pub(crate) slots: Vec<SlotInfo>,
    /// The slot named `namespace`: the default context a reference whose
    /// qualification may come from context captures where it binds, when
    /// neither its reference type nor its field names another slot
    /// through `qualify-from`.
    pub(crate) namespace: Option<SlotId>,
}

/// One context slot, as the binder sees it.
#[derive(Clone, Debug)]
pub(crate) struct SlotInfo {
    pub(crate) name: Sym,
    pub(crate) ty: TypeExpr,
    pub(crate) accumulate: Option<crate::definition::AccumulateOp>,
    /// How a `merge` accumulation treats a key contributed twice.
    pub(crate) merge_duplicates: crate::definition::MergeDuplicates,
    /// How a plain set reaching the occupied slot is treated.
    pub(crate) duplicates: crate::definition::SlotDuplicates,
    /// The layer order of a layered slot, highest precedence first.
    pub(crate) layers: Vec<String>,
}

/// The context slot naming the qualifier source in effect at a position
/// when nothing more specific names one: the default a reference type's
/// or a field's `qualify-from` overrides.
const NAMESPACE_SLOT: &str = "namespace";

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
        namespace: def
            .names
            .get(NAMESPACE_SLOT)
            .and_then(|name| def.plan.context.slot(name)),
        decls: Vec::with_capacity(def.bindings.len()),
        scopes: vec![None; def.bindings.scopes().len()],
        slots: slots
            .into_iter()
            .map(|(_, name, ty)| {
                let decl = def.slots.iter().find(|slot| slot.name == name);
                SlotInfo {
                    name,
                    ty,
                    accumulate: decl.and_then(|slot| slot.accumulate),
                    // A slot the plan lays out always has a declaration;
                    // were one missing, the fail-closed defaults apply.
                    merge_duplicates: decl.map(|slot| slot.merge_duplicates).unwrap_or_default(),
                    duplicates: decl.map(|slot| slot.duplicates).unwrap_or_default(),
                    layers: decl
                        .map(|slot| {
                            slot.layers
                                .iter()
                                .map(|layer| def.names.resolve(*layer).to_owned())
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect(),
    };
    for (id, decl) in def.bindings.iter() {
        let info = decl_info(def, id, decl);
        let mut uses: Vec<Use<'_>> = Vec::new();
        for binding in decl
            .fields
            .iter()
            .chain(decl.cases.iter().flat_map(|case| &case.fields))
        {
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
            Source::Child { name, sources, .. } | Source::Children { name, sources, .. } => {
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
                case: None,
                ty: Some(field.ty.clone()),
                required: !optional(&field.ty),
                collect: Collect::No,
                // Only a literal fallback supplies an absent value; a
                // computed default is a descriptor the engine never
                // computes.
                fallback: field.fallback.as_ref().and_then(|d| d.literal().cloned()),
                computed: match &field.fallback {
                    Some(DefaultSpec::Computed { dependencies }) => Some(dependencies.clone()),
                    _ => None,
                },
                context: None,
                sensitivity: field.sensitivity,
                expand: Vec::new(),
                reference: reference_type(def, &field.ty),
                qualify: None,
            })
            .collect(),
        None => decl
            .fields
            .iter()
            .map(|binding| FieldInfo {
                name: binding.name,
                case: None,
                ty: binding.ty.clone(),
                required: binding.ty.as_ref().is_some_and(|ty| !optional(ty)),
                collect: Collect::No,
                fallback: None,
                computed: None,
                context: None,
                sensitivity: None,
                expand: Vec::new(),
                reference: binding.ty.as_ref().and_then(|ty| reference_type(def, ty)),
                qualify: None,
            })
            .collect(),
    };
    // A case-level field joins the declaration's field set once, under
    // the first case binding its name; its supplies are gated by the
    // case the binding belongs to.
    for (case_at, case) in decl.cases.iter().enumerate() {
        for binding in &case.fields {
            if let Some(at) = fields.iter().position(|info| info.name == binding.name) {
                // A typed node's object supplies the field; it binds only
                // under this case while no common binding takes it.
                if fields[at].case.is_none()
                    && !decl.fields.iter().any(|common| common.name == binding.name)
                {
                    fields[at].case = Some(case_at);
                }
                continue;
            }
            let from_object = object.and_then(|object| {
                object
                    .fields
                    .iter()
                    .find(|field| field.name == binding.name)
            });
            let info = match from_object {
                Some(field) => FieldInfo {
                    name: field.name,
                    case: Some(case_at),
                    ty: Some(field.ty.clone()),
                    required: !optional(&field.ty),
                    collect: Collect::No,
                    fallback: field.fallback.as_ref().and_then(|d| d.literal().cloned()),
                    computed: match &field.fallback {
                        Some(DefaultSpec::Computed { dependencies }) => Some(dependencies.clone()),
                        _ => None,
                    },
                    context: None,
                    sensitivity: field.sensitivity,
                    expand: Vec::new(),
                    reference: reference_type(def, &field.ty),
                    qualify: None,
                },
                None => FieldInfo {
                    name: binding.name,
                    case: Some(case_at),
                    ty: binding.ty.clone(),
                    required: binding.ty.as_ref().is_some_and(|ty| !optional(ty)),
                    collect: Collect::No,
                    fallback: None,
                    computed: None,
                    context: None,
                    sensitivity: None,
                    expand: Vec::new(),
                    reference: binding.ty.as_ref().and_then(|ty| reference_type(def, ty)),
                    qualify: None,
                },
            };
            fields.push(info);
        }
    }
    let mut path = None;
    let mut case_bindings: Vec<&crate::bindings::FieldBinding> = decl
        .cases
        .iter()
        .flat_map(|case| case.fields.iter())
        .collect();
    for binding in decl.fields.iter().chain(case_bindings.drain(..)) {
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
                // A case-level binding never identifies an import path;
                // the import's own bindings stay common.
                Source::Argument(0) if info.case.is_none() => path = Some(at),
                _ => {}
            }
        }
        // The field's own `qualify-from` wins over its reference type's;
        // either displaces the definition's default slot.
        if info.qualify.is_none() {
            let slot = binding
                .qualify_from
                .or(info.reference.as_ref().and_then(|decl| decl.qualify_from));
            if let Some(slot) = slot {
                info.qualify = def.plan.context.slot(slot);
            }
        }
    }
    let position = |name: Sym| fields.iter().position(|f| f.name == name);
    let import = decl.role == Some(Role::SourceImport);
    let import_field = |name: &str| def.names.get(name).and_then(position).filter(|_| import);
    let contribution = decl.role == Some(Role::Contribution);
    let contribution_field = |name: &str| {
        def.names
            .get(name)
            .and_then(position)
            .filter(|_| contribution)
    };
    let identity = match def.symbols.declaration(id) {
        Some(declaration) if declaration.identity.is_some() => IdentityBind::Composed {
            domain: def.names.resolve(declaration.domain).to_owned(),
            fields: declaration
                .identity_fields
                .iter()
                .map(|name| (*name, position(*name)))
                .collect(),
        },
        Some(declaration) => IdentityBind::Fallback(
            declaration
                .identity_fields
                .first()
                .copied()
                .and_then(position),
        ),
        None => IdentityBind::Fallback(None),
    };
    // Declared-reference-typed fields are added by the per-field
    // `reference_type` scan below, in field order; only `ref<T>` plan
    // entries seed the list.
    let deferred: Vec<&DeferredBinding> = def
        .plan
        .node(id)
        .map_or(&[][..], |plan| &plan.deferred)
        .iter()
        .filter(|binding| {
            !matches!(
                binding,
                DeferredBinding::Reference {
                    reference: Some(_),
                    ..
                }
            )
        })
        .collect();
    let mut references: Vec<usize> = deferred
        .iter()
        .filter_map(|binding| match binding {
            DeferredBinding::Reference { field, .. } => position(*field),
            DeferredBinding::Contribution => None,
        })
        .collect();
    for (at, field) in fields.iter().enumerate() {
        if field.reference.is_some() && !references.contains(&at) {
            references.push(at);
        }
    }
    DeclInfo {
        type_name: def.names.resolve(decl.ty.unwrap_or(decl.name)).to_owned(),
        identity,
        path: path.filter(|_| import),
        project: import_field("project"),
        repeated: import_field("repeated"),
        namespace: import_field("namespace"),
        layer: contribution_field("layer"),
        scope: contribution_field("scope"),
        provisional: !deferred.is_empty() || !references.is_empty(),
        references,
        cases: decl.cases.clone(),
        otherwise: decl.otherwise.clone(),
        dependent: decl.value.clone(),
        path_value: decl.path_value.clone(),
        fields,
    }
}

/// The declared reference type a field's type names, through any
/// optional, nullable, or list wrapper, or as its map's key type.
fn reference_type(def: &Definition, ty: &TypeExpr) -> Option<ReferenceDecl> {
    match ty {
        TypeExpr::Named(name) => def.symbols.reference(*name).cloned(),
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) | TypeExpr::List(inner) => {
            reference_type(def, inner)
        }
        TypeExpr::Map(key, _) | TypeExpr::OrderedMap(key, _) => reference_type(def, key),
        _ => None,
    }
}

pub(crate) fn optional(ty: &TypeExpr) -> bool {
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
