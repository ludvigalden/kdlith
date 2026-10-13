//! Node declarations compiled into dispatch and per-scope plans; rejects
//! overlapping bindings, unconsumed arguments, and contradictory child
//! ownership.

use std::collections::BTreeMap;

use crate::bindings::{
    Arity, BindingCatalog, ChildPlan, ChildRule, NodeDecl, NodeDeclId, ParentKey, Placement, Scope,
    ScopeId, Source,
};
use crate::compile::Cx;
use crate::diag::{CONFLICTING_BINDING, INVALID_BINDING, Site, duplicate};
use crate::names::Sym;
use crate::structure::{Occurs, OrderRule, ScopeRules};

/// One binding source with the field it populates.
struct Use<'s> {
    field: Sym,
    site: Site,
    source: &'s Source,
}

/// How one field consumes a child name.
struct ChildUse<'s> {
    field: Sym,
    site: Site,
    many: bool,
    nested: &'s [Source],
    entries: Option<(u64, Sym)>,
}

/// What a scope is built from.
struct ScopeInput<'s> {
    owner: Option<Sym>,
    decl: Option<NodeDeclId>,
    label: String,
    site: Site,
    uses: Vec<Use<'s>>,
    rules: Vec<ChildRule>,
    groups: Vec<Vec<Sym>>,
    keys: Vec<u64>,
}

pub(crate) fn build(cx: &mut Cx<'_>) -> BindingCatalog {
    let nodes = std::mem::take(&mut cx.nodes);
    let dispatch = dispatch_table(cx, &nodes);
    let keys = entry_keys(&nodes);
    let mut builder = Builder {
        nodes: &nodes,
        dispatch: &dispatch,
        scopes: Vec::new(),
    };
    let mut decl_scopes = Vec::with_capacity(nodes.len());
    for (at, decl) in nodes.iter().enumerate() {
        let uses = decl
            .fields
            .iter()
            .flat_map(|field| {
                field.sources.iter().map(|source| Use {
                    field: field.name,
                    site: field.site,
                    source,
                })
            })
            .collect();
        let input = ScopeInput {
            owner: Some(decl.name),
            decl: Some(NodeDeclId::from_index(at)),
            label: cx.names.resolve(decl.name).to_owned(),
            site: decl.site,
            uses,
            rules: decl.children.clone(),
            groups: decl.groups.clone(),
            keys: keys.get(&at).cloned().unwrap_or_default(),
        };
        decl_scopes.push(builder.scope(cx, input));
    }
    let root = builder.root(cx);
    BindingCatalog {
        scopes: builder.scopes,
        nodes,
        dispatch,
        decl_scopes,
        root,
    }
}

/// Index every declaration by (parent, name); a pair declared twice is a
/// duplicate declaration.
fn dispatch_table(cx: &mut Cx<'_>, nodes: &[NodeDecl]) -> BTreeMap<(ParentKey, Sym), NodeDeclId> {
    let mut dispatch = BTreeMap::new();
    for (at, decl) in nodes.iter().enumerate() {
        let parents: Vec<ParentKey> = match &decl.placement {
            Placement::Root => vec![ParentKey::Root],
            Placement::Under(parents) => parents.iter().map(|p| ParentKey::Under(*p)).collect(),
        };
        for parent in parents {
            if let Some(earlier) = dispatch.insert((parent, decl.name), NodeDeclId::from_index(at))
            {
                dispatch.insert((parent, decl.name), earlier);
                let first = nodes[earlier.index()].site;
                let name = cx.names.resolve(decl.name).to_owned();
                let spelled = match parent {
                    ParentKey::Root => format!("{name} at the root"),
                    ParentKey::Under(p) => format!("{name} under {}", cx.names.resolve(p)),
                };
                duplicate(&mut cx.diags, cx.kind, (decl.site, first), "node", &spelled);
            }
        }
    }
    dispatch
}

/// The arguments entries envelopes consume as keys, per target
/// declaration: every declaration placed under an envelope's owner with
/// the entry node's name, duplicates included, so a rejected duplicate
/// reports only its duplication.
fn entry_keys(nodes: &[NodeDecl]) -> BTreeMap<usize, Vec<u64>> {
    let mut keys: BTreeMap<usize, Vec<u64>> = BTreeMap::new();
    for owner in nodes {
        for source in owner.fields.iter().flat_map(|f| &f.sources) {
            let Source::Entries { node, key, .. } = source else {
                continue;
            };
            for (at, target) in nodes.iter().enumerate() {
                let placed = matches!(&target.placement, Placement::Under(parents) if parents.contains(&owner.name));
                if target.name == *node && placed {
                    let list = keys.entry(at).or_default();
                    if !list.contains(key) {
                        list.push(*key);
                    }
                }
            }
        }
    }
    keys
}

struct Builder<'n> {
    nodes: &'n [NodeDecl],
    dispatch: &'n BTreeMap<(ParentKey, Sym), NodeDeclId>,
    scopes: Vec<Scope>,
}

impl Builder<'_> {
    fn lookup(&self, owner: Option<Sym>, name: Sym) -> Option<NodeDeclId> {
        let parent = owner.map_or(ParentKey::Root, ParentKey::Under);
        self.dispatch.get(&(parent, name)).copied()
    }

    fn root(&mut self, cx: &mut Cx<'_>) -> ScopeId {
        let mut rules = ScopeRules::default();
        let mut plans = BTreeMap::new();
        for ((parent, name), decl) in self.dispatch {
            if *parent == ParentKey::Root {
                rules.permit(Occurs {
                    name: *name,
                    min: 0,
                    max: None,
                });
                plans.insert(
                    *name,
                    ChildPlan::Declared {
                        decl: *decl,
                        key: None,
                    },
                );
            }
        }
        if let Some(rule) = &cx.document_rule {
            rules.order(OrderRule {
                first: rule.first.clone(),
                following: rule.following.clone(),
            });
        }
        self.push(Scope {
            name: None,
            decl: None,
            rules,
            arity: Arity::default(),
            properties: Vec::new(),
            plans,
        })
    }

    fn push(&mut self, scope: Scope) -> ScopeId {
        let id = ScopeId::from_index(self.scopes.len());
        self.scopes.push(scope);
        id
    }

    fn scope(&mut self, cx: &mut Cx<'_>, input: ScopeInput<'_>) -> ScopeId {
        let id = self.push(Scope {
            name: input.owner,
            decl: input.decl,
            rules: ScopeRules::default(),
            arity: Arity::default(),
            properties: Vec::new(),
            plans: BTreeMap::new(),
        });
        let arity = arguments(cx, &input);
        let properties = properties(cx, &input);
        let (plans, rules) = self.children(cx, &input);
        let scope = &mut self.scopes[id.index()];
        scope.arity = arity;
        scope.properties = properties;
        scope.plans = plans;
        scope.rules = rules;
        id
    }

    /// Plan every consumed or ruled child and compile the counters.
    fn children(
        &mut self,
        cx: &mut Cx<'_>,
        input: &ScopeInput<'_>,
    ) -> (BTreeMap<Sym, ChildPlan>, ScopeRules) {
        let mut order: Vec<Sym> = Vec::new();
        let mut uses: BTreeMap<Sym, Vec<ChildUse<'_>>> = BTreeMap::new();
        for usage in &input.uses {
            let (name, child) = match usage.source {
                Source::Child { name, sources } | Source::Children { name, sources } => (
                    *name,
                    ChildUse {
                        field: usage.field,
                        site: usage.site,
                        many: matches!(usage.source, Source::Children { .. }),
                        nested: sources,
                        entries: None,
                    },
                ),
                Source::Entries { node, key, value } => (
                    *node,
                    ChildUse {
                        field: usage.field,
                        site: usage.site,
                        many: true,
                        nested: &[],
                        entries: Some((*key, *value)),
                    },
                ),
                _ => continue,
            };
            if !order.contains(&name) {
                order.push(name);
            }
            uses.entry(name).or_default().push(child);
        }
        for rule in &input.rules {
            if !order.contains(&rule.name) {
                order.push(rule.name);
            }
        }
        let mut plans = BTreeMap::new();
        let mut rules = ScopeRules::default();
        for name in order {
            let rule = input.rules.iter().find(|r| r.name == name);
            let consumed = uses.remove(&name).unwrap_or_default();
            let Some((plan, single)) = self.plan(cx, input, name, &consumed, rule) else {
                continue;
            };
            plans.insert(name, plan);
            let max = rule.and_then(|r| r.max).or(single.then_some(1));
            rules.permit(Occurs {
                name,
                min: rule.and_then(|r| r.min).unwrap_or(0),
                max,
            });
        }
        for group in &input.groups {
            for missing in rules.group(group) {
                let reason = format!(
                    "exactly-one names {}, which is not a permitted child",
                    cx.names.resolve(missing)
                );
                cx.diags
                    .error(INVALID_BINDING, input.site, &[&input.label, &reason]);
            }
        }
        (plans, rules)
    }

    /// The plan for one child name and whether it binds at most once.
    fn plan(
        &mut self,
        cx: &mut Cx<'_>,
        input: &ScopeInput<'_>,
        name: Sym,
        consumed: &[ChildUse<'_>],
        rule: Option<&ChildRule>,
    ) -> Option<(ChildPlan, bool)> {
        let spelled = cx.names.resolve(name).to_owned();
        let declared = self.lookup(input.owner, name);
        let invalid = |cx: &mut Cx<'_>, reason: String| {
            cx.diags
                .error(INVALID_BINDING, input.site, &[&input.label, &reason]);
        };
        if consumed.is_empty() {
            let Some(decl) = declared else {
                invalid(
                    cx,
                    format!(
                        "children rule names {spelled}, which no field consumes and no node declares here"
                    ),
                );
                return None;
            };
            return Some((ChildPlan::Declared { decl, key: None }, false));
        }
        let syntax = format!("child {spelled}");
        let whole: Vec<&ChildUse<'_>> = consumed.iter().filter(|u| u.nested.is_empty()).collect();
        if whole.len() > 1 || (!whole.is_empty() && whole.len() < consumed.len()) {
            let first = cx.names.resolve(consumed[0].field).to_owned();
            let other = cx.names.resolve(consumed[1].field).to_owned();
            cx.diags.error(
                CONFLICTING_BINDING,
                consumed[1].site,
                &[&syntax, &first, &other],
            );
            return None;
        }
        let many = consumed[0].many;
        if consumed.iter().any(|u| u.many != many) {
            invalid(
                cx,
                format!("{syntax} is bound both once and as repeated children"),
            );
            return None;
        }
        if !many && rule.is_some_and(|r| r.max.is_none_or(|max| max > 1)) {
            invalid(
                cx,
                format!("{syntax} is bound once but its children rule permits more"),
            );
            return None;
        }
        if let Some((key, value)) = consumed[0].entries {
            let Some(decl) = declared else {
                invalid(
                    cx,
                    format!("entries node {spelled} has no declaration here"),
                );
                return None;
            };
            if self.nodes[decl.index()].ty != Some(value) {
                let value = cx.names.resolve(value);
                invalid(
                    cx,
                    format!("entries value {value} is not the type node {spelled} declares"),
                );
                return None;
            }
            return Some((
                ChildPlan::Declared {
                    decl,
                    key: Some(key),
                },
                false,
            ));
        }
        if !whole.is_empty() {
            let plan = match declared {
                Some(decl) => ChildPlan::Declared { decl, key: None },
                None => ChildPlan::Scalar,
            };
            return Some((plan, !many));
        }
        let mut nested = ScopeInput {
            owner: Some(name),
            decl: None,
            label: format!("{} > {spelled}", input.label),
            site: input.site,
            uses: consumed
                .iter()
                .flat_map(|u| {
                    u.nested.iter().map(|source| Use {
                        field: u.field,
                        site: u.site,
                        source,
                    })
                })
                .collect(),
            rules: Vec::new(),
            groups: Vec::new(),
            keys: Vec::new(),
        };
        if let Some(decl) = declared {
            let node = &self.nodes[decl.index()];
            if !node.fields.is_empty() || node.role.is_some() || node.ty.is_some() {
                invalid(
                    cx,
                    format!(
                        "{syntax} is selected by nested sources and also bound through its own declaration"
                    ),
                );
                return None;
            }
            nested.rules.clone_from(&node.children);
            nested.groups.clone_from(&node.groups);
        }
        Some((ChildPlan::Nested(self.scope(cx, nested)), !many))
    }
}

/// Positional arguments: each index bound once, no gaps below the
/// highest, and at most one collector of the rest.
fn arguments(cx: &mut Cx<'_>, input: &ScopeInput<'_>) -> Arity {
    const KEY: &str = "the entries key";
    let mut bound: BTreeMap<u64, String> = BTreeMap::new();
    for key in &input.keys {
        bound.insert(*key, KEY.to_owned());
    }
    let mut rest: Option<Sym> = None;
    for usage in &input.uses {
        let field = cx.names.resolve(usage.field).to_owned();
        match usage.source {
            Source::Argument(index) => {
                if let Some(other) = bound.get(index) {
                    let syntax = format!("argument {index}");
                    cx.diags
                        .error(CONFLICTING_BINDING, usage.site, &[&syntax, other, &field]);
                    continue;
                }
                bound.insert(*index, field);
            }
            Source::Args => {
                if let Some(other) = rest {
                    let other = cx.names.resolve(other).to_owned();
                    cx.diags.error(
                        CONFLICTING_BINDING,
                        usage.site,
                        &["the remaining arguments", &other, &field],
                    );
                    continue;
                }
                rest = Some(usage.field);
            }
            _ => {}
        }
    }
    let indexed = bound
        .keys()
        .next_back()
        .map_or(0, |last| last.saturating_add(1));
    if indexed > bound.len() as u64 {
        for index in (0..indexed).filter(|i| !bound.contains_key(i)) {
            let reason =
                format!("argument {index} is not consumed while a later argument is bound");
            cx.diags
                .error(INVALID_BINDING, input.site, &[&input.label, &reason]);
        }
    }
    Arity {
        indexed,
        rest: rest.is_some(),
    }
}

fn properties(cx: &mut Cx<'_>, input: &ScopeInput<'_>) -> Vec<Sym> {
    let mut bound: BTreeMap<Sym, Sym> = BTreeMap::new();
    for usage in &input.uses {
        let Source::Property(name) = usage.source else {
            continue;
        };
        if let Some(other) = bound.get(name) {
            let syntax = format!("property {}", cx.names.resolve(*name));
            let other = cx.names.resolve(*other).to_owned();
            let field = cx.names.resolve(usage.field).to_owned();
            cx.diags
                .error(CONFLICTING_BINDING, usage.site, &[&syntax, &other, &field]);
            continue;
        }
        bound.insert(*name, usage.field);
    }
    bound.into_keys().collect()
}
