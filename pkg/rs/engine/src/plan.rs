//! The ProcessingPlan, compiled once from linked catalogs: the numbered
//! context layout, resolved effects, context reads, deferred bindings,
//! and the scope whose completion checks run when each node closes.

use std::collections::BTreeMap;

use kdlith_runtime::{ContextLayout, NamespaceId, SlotId};

use crate::bindings::{
    BindingCatalog, ContextPolicy, Effect, EffectTarget, FieldBinding, NodeDecl, Role, ScopeId,
    SetFrom, Source,
};
use crate::definition::SlotDecl;
use crate::names::Sym;
use crate::symbols::SymbolCatalog;
use crate::types::{TypeCatalog, TypeExpr, TypeKind};

/// The context slots of a definition, numbered for the runtime.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContextPlan {
    pub layout: ContextLayout,
    slots: BTreeMap<Sym, SlotId>,
    namespaces: BTreeMap<Sym, NamespaceId>,
}

impl ContextPlan {
    pub fn slot(&self, name: Sym) -> Option<SlotId> {
        self.slots.get(&name).copied()
    }

    pub fn namespace(&self, name: Sym) -> Option<NamespaceId> {
        self.namespaces.get(&name).copied()
    }
}

/// One context effect with its slot or namespace resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EffectStep {
    Set {
        slot: SlotId,
        from: SetFrom,
        /// The layer a set on a layered slot names.
        layer: Option<Sym>,
    },
    Remove {
        slot: SlotId,
    },
    /// Remove the slots the named field's string values name, per
    /// invocation; each name resolves against the declared slots at the
    /// effect point, failing closed when one names none.
    RemoveArgs {
        field: Sym,
    },
    Clear {
        namespace: NamespaceId,
    },
    /// Domain computation the installing processor implements.
    Handler {
        name: String,
    },
}

/// The effect steps of one group: a node's effects for one target,
/// with the group's replace property — its set effects replace the
/// held value of the slots they name instead of failing the
/// duplicate-set check, and so do the sets a handler splices in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EffectSteps {
    pub steps: Vec<EffectStep>,
    pub replace: bool,
}

/// A field read from a context slot under its policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContextRead {
    pub field: Sym,
    pub slot: SlotId,
    pub policy: Option<ContextPolicy>,
}

/// Work a node leaves for complete-scope processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeferredBinding {
    /// A field resolved by name among declarations of `target`: typed
    /// `ref<T>` when `reference` is `None`, else typed by the declared
    /// reference type `reference` names, whose target is `target`.
    Reference {
        field: Sym,
        target: Sym,
        reference: Option<Sym>,
    },
    /// The node's bound result is preserved for later resolution.
    Contribution,
}

/// What a session does around one declared node.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NodePlan {
    pub role: Option<Role>,
    /// The scope whose rules admit the node's children and whose
    /// completion checks run when it closes.
    pub scope: Option<ScopeId>,
    /// Applied when the node's children open; computable from its header.
    pub children_effects: EffectSteps,
    /// Applied to later siblings once the node completes successfully.
    pub following_effects: EffectSteps,
    pub context_reads: Vec<ContextRead>,
    pub deferred: Vec<DeferredBinding>,
    /// The dependent payload binding the node declares, if any: the
    /// plan a processor follows to bind its remaining payload against
    /// the referenced declaration's member type.
    pub dependent: Option<crate::bindings::DependentBinding>,
    /// Whether the node registers an identity in the SymbolCatalog.
    pub declaration: bool,
}

/// The processing plan of one definition, indexed like its
/// BindingCatalog. Expansion references resolve during complete
/// checking, before the plan is built.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProcessingPlan {
    pub context: ContextPlan,
    nodes: Vec<NodePlan>,
}

impl ProcessingPlan {
    pub fn node(&self, id: crate::bindings::NodeDeclId) -> Option<&NodePlan> {
        self.nodes.get(id.index())
    }

    pub fn nodes(&self) -> &[NodePlan] {
        &self.nodes
    }
}

/// Build the plan from linked catalogs; every name it looks up was
/// checked by linking, so nothing is dropped here.
pub(crate) fn build(
    slots: &[SlotDecl],
    types: &TypeCatalog,
    bindings: &BindingCatalog,
    symbols: &SymbolCatalog,
) -> ProcessingPlan {
    let context = context_plan(slots);
    let nodes = bindings
        .iter()
        .map(|(id, node)| NodePlan {
            role: node.role,
            scope: bindings.decl_scope(id),
            children_effects: effects(&context, node, EffectTarget::Children),
            following_effects: effects(&context, node, EffectTarget::Following),
            context_reads: context_reads(&context, node),
            deferred: deferred(types, symbols, node),
            dependent: node.value.clone(),
            declaration: symbols.declaration(id).is_some(),
        })
        .collect();
    ProcessingPlan { context, nodes }
}

fn context_plan(slots: &[SlotDecl]) -> ContextPlan {
    let mut plan = ContextPlan::default();
    for slot in slots {
        if let Some(namespace) = slot.namespace
            && !plan.namespaces.contains_key(&namespace)
        {
            let id = plan.layout.namespace();
            plan.namespaces.insert(namespace, id);
        }
    }
    for slot in slots {
        let namespace = slot
            .namespace
            .and_then(|ns| plan.namespaces.get(&ns).copied());
        if let Some(id) = plan.layout.slot(namespace) {
            plan.slots.insert(slot.name, id);
        }
    }
    plan
}

fn effects(context: &ContextPlan, node: &NodeDecl, target: EffectTarget) -> EffectSteps {
    let mut steps = Vec::new();
    // Linking rejects a second group for one target, so the steps of a
    // target come from one group and carry its replace property.
    let mut replace = false;
    for group in node.effects.iter().filter(|group| group.target == target) {
        replace = group.replace;
        for effect in &group.effects {
            let step = match effect {
                Effect::Set { slot, from, layer } => {
                    context.slot(*slot).map(|slot| EffectStep::Set {
                        slot,
                        from: from.clone(),
                        layer: *layer,
                    })
                }
                Effect::Remove { slot } => {
                    context.slot(*slot).map(|slot| EffectStep::Remove { slot })
                }
                Effect::RemoveArgs { field } => Some(EffectStep::RemoveArgs { field: *field }),
                Effect::Clear { namespace } => context
                    .namespace(*namespace)
                    .map(|namespace| EffectStep::Clear { namespace }),
                Effect::Handler { name } => Some(EffectStep::Handler { name: name.clone() }),
            };
            steps.extend(step);
        }
    }
    EffectSteps { steps, replace }
}

fn context_reads(context: &ContextPlan, node: &NodeDecl) -> Vec<ContextRead> {
    let mut reads = Vec::new();
    for field in &node.fields {
        for source in &field.sources {
            if let Source::Context { slot, policy } = source
                && let Some(slot) = context.slot(*slot)
            {
                reads.push(ContextRead {
                    field: field.name,
                    slot,
                    policy: *policy,
                });
            }
        }
    }
    reads
}

fn deferred(types: &TypeCatalog, symbols: &SymbolCatalog, node: &NodeDecl) -> Vec<DeferredBinding> {
    let mut out = Vec::new();
    if node.role == Some(Role::Contribution) {
        out.push(DeferredBinding::Contribution);
    }
    for field in &node.fields {
        if let Some(expr) = field_type(types, node, field) {
            references(symbols, expr, field.name, &mut out);
        }
    }
    out
}

/// A binding's type: inline, or the node type's field of that name.
pub(crate) fn field_type<'t>(
    types: &'t TypeCatalog,
    node: &NodeDecl,
    field: &'t FieldBinding,
) -> Option<&'t TypeExpr> {
    if let Some(inline) = &field.ty {
        return Some(inline);
    }
    let decl = types.get(types.lookup(node.ty?)?)?;
    let TypeKind::Object(object) = &decl.kind else {
        return None;
    };
    object
        .fields
        .iter()
        .find(|def| def.name == field.name)
        .map(|def| &def.ty)
}

/// The references a field's type holds. A generic instantiation is not
/// walked, matching the link stage's field typing.
fn references(
    symbols: &SymbolCatalog,
    expr: &TypeExpr,
    field: Sym,
    out: &mut Vec<DeferredBinding>,
) {
    match expr {
        TypeExpr::Named(name) => {
            if let Some(decl) = symbols.reference(*name) {
                out.push(DeferredBinding::Reference {
                    field,
                    target: decl.target,
                    reference: Some(*name),
                });
            }
        }
        TypeExpr::Instantiation(_, _) => {}
        TypeExpr::Ref(inner) => {
            if let TypeExpr::Named(target) = inner.as_ref() {
                out.push(DeferredBinding::Reference {
                    field,
                    target: *target,
                    reference: None,
                });
            }
        }
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) | TypeExpr::List(inner) => {
            references(symbols, inner, field, out);
        }
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            references(symbols, key, field, out);
            references(symbols, value, field, out);
        }
        TypeExpr::Union(members) => {
            for member in members {
                references(symbols, member, field, out);
            }
        }
    }
}
