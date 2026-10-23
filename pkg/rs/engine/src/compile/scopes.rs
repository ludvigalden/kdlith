//! Node declarations compiled into dispatch and per-scope plans; rejects
//! overlapping bindings (through the shared overlap check), unconsumed
//! arguments, and contradictory child ownership.

use std::collections::{BTreeMap, BTreeSet};

use crate::bindings::{
    Arity, BindingCatalog, ChildPlan, ChildRule, DescendantRule, NodeDecl, NodeDeclId, ParentKey,
    Scope, ScopeId, SequenceDecl, SequenceStep, Source,
};
use crate::compile::Cx;
use crate::definition::CheckScope;
use crate::diag::{CONFLICTING_BINDING, INVALID_BINDING, Site, duplicate};
use crate::names::Sym;
use crate::overlap::{Consumed, overlaps};
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
    compact: bool,
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
    descendants: Vec<DescendantRule>,
    sequence: Option<SequenceDecl>,
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
            descendants: decl.descendants.clone(),
            sequence: decl.sequence.clone(),
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
        let root = decl.placement.at_root().then_some(ParentKey::Root);
        let under = decl
            .placement
            .parents()
            .iter()
            .map(|p| ParentKey::Under(*p));
        let parents: Vec<ParentKey> = root.into_iter().chain(under).collect();
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
                let placed = target.placement.parents().contains(&owner.name);
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
    /// Whether any declaration names `name`, so a descendant rule can
    /// range over it.
    fn known(&self, name: Sym) -> bool {
        self.nodes.iter().any(|decl| decl.name == name)
    }

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
            compact: Vec::new(),
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
            compact: Vec::new(),
        });
        let (arity, properties) = arguments_and_properties(cx, &input);
        let (plans, mut rules, compact) = self.children(cx, &input);
        for name in &compact {
            if properties.contains(name) {
                let spelled = cx.names.resolve(*name);
                let reason = format!(
                    "property {spelled} is bound and also spells the compact child {spelled}"
                );
                cx.malformed(input.site, &input.label, &reason);
            }
        }
        let sequenced = input
            .sequence
            .as_ref()
            .and_then(|sequence| compile_sequence(cx, &input, &rules, sequence));
        if let Some(steps) = sequenced {
            rules.set_sequence(steps);
        }
        for rule in &input.descendants {
            if !self.known(rule.name) {
                let spelled = cx.names.resolve(rule.name).to_owned();
                let reason =
                    format!("descendants names {spelled}, which no node declaration names");
                cx.diags
                    .error(INVALID_BINDING, input.site, &[&input.label, &reason]);
                continue;
            }
            rules.descendant(Occurs {
                name: rule.name,
                min: rule.min.unwrap_or(0),
                max: rule.max,
            });
        }
        let scope = &mut self.scopes[id.index()];
        scope.arity = arity;
        scope.properties = properties;
        scope.plans = plans;
        scope.rules = rules;
        scope.compact = compact;
        id
    }

    /// Plan every consumed or ruled child and compile the counters; the
    /// children admitting the compact spelling come back with them.
    fn children(
        &mut self,
        cx: &mut Cx<'_>,
        input: &ScopeInput<'_>,
    ) -> (BTreeMap<Sym, ChildPlan>, ScopeRules, Vec<Sym>) {
        let mut order: Vec<Sym> = Vec::new();
        let mut uses: BTreeMap<Sym, Vec<ChildUse<'_>>> = BTreeMap::new();
        for usage in &input.uses {
            let (name, child) = match usage.source {
                Source::Child {
                    name,
                    sources,
                    compact,
                }
                | Source::Children {
                    name,
                    sources,
                    compact,
                } => (
                    *name,
                    ChildUse {
                        field: usage.field,
                        site: usage.site,
                        many: matches!(usage.source, Source::Children { .. }),
                        nested: sources,
                        entries: None,
                        compact: *compact,
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
                        compact: false,
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
        let mut compact = Vec::new();
        for name in order {
            let rule = input.rules.iter().find(|r| r.name == name);
            let consumed = uses.remove(&name).unwrap_or_default();
            let Some((plan, single)) = self.plan(cx, input, name, &consumed, rule) else {
                continue;
            };
            if let Some(usage) = consumed.iter().find(|u| u.compact) {
                match self.compact_refusal(cx, plan, &consumed) {
                    None => compact.push(name),
                    Some(reason) => {
                        let spelled = cx.names.resolve(name);
                        let construct = format!("compact child {spelled} of {}", input.label);
                        cx.malformed(usage.site, &construct, &reason);
                    }
                }
            }
            plans.insert(name, plan);
            let max = rule.and_then(|r| r.max).or(single.then_some(1));
            let occurs = Occurs {
                name,
                min: rule.and_then(|r| r.min).unwrap_or(0),
                max,
            };
            match rule.filter(|r| !r.branches.is_empty()) {
                Some(rule) => rules.permit_conditional(occurs, rule.branches.clone()),
                None => rules.permit(occurs),
            }
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
        (plans, rules, compact)
    }

    /// Why a child cannot be spelled as a property carrying its one
    /// argument: every field it binds must come from that argument alone,
    /// with nothing else of the child's to bind or dispatch on.
    fn compact_refusal(
        &self,
        cx: &Cx<'_>,
        plan: ChildPlan,
        consumed: &[ChildUse<'_>],
    ) -> Option<String> {
        let only_first = |sources: &[Source]| {
            !sources.is_empty() && sources.iter().all(|s| *s == Source::Argument(0))
        };
        match plan {
            ChildPlan::Scalar => None,
            ChildPlan::Nested(_) => {
                let nested: Vec<Source> = consumed
                    .iter()
                    .flat_map(|u| u.nested.iter().cloned())
                    .collect();
                (!only_first(&nested))
                    .then(|| "its nested sources select more than its argument 0".to_owned())
            }
            ChildPlan::Declared { key: Some(_), .. } => {
                Some("an entries node keys its map by its own argument".to_owned())
            }
            ChildPlan::Declared { decl, key: None } => {
                let node = &self.nodes[decl.index()];
                let shaped = node.role.is_some()
                    || node.value.is_some()
                    || !node.cases.is_empty()
                    || node.otherwise.is_some()
                    || !node.children.is_empty()
                    || !node.groups.is_empty()
                    || !node.descendants.is_empty()
                    || node.sequence.is_some()
                    || !node.effects.is_empty();
                if shaped {
                    return Some(
                        "its declaration carries more than field bindings: a role, dispatch, effects, or children rules"
                            .to_owned(),
                    );
                }
                if node.fields.is_empty() {
                    return Some("its declaration binds no field".to_owned());
                }
                node.fields
                    .iter()
                    .find(|field| !only_first(&field.sources))
                    .map(|field| {
                        format!(
                            "field {} binds from more than argument 0",
                            cx.names.resolve(field.name)
                        )
                    })
            }
        }
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
                // Another contributing document may declare it; only
                // complete checking sees every contribution.
                if cx.scope == CheckScope::Local {
                    return None;
                }
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
            descendants: Vec::new(),
            sequence: None,
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

/// Arguments and properties two fields bind (with an entries envelope's
/// key counted as consumed) are reported as overlaps; the first binding
/// of each piece of syntax is kept.
fn arguments_and_properties(cx: &mut Cx<'_>, input: &ScopeInput<'_>) -> (Arity, Vec<Sym>) {
    const KEY: &str = "the entries key";
    let names: Vec<String> = input
        .uses
        .iter()
        .map(|usage| cx.names.resolve(usage.field).to_owned())
        .collect();
    let mut sites = Vec::new();
    let mut uses = Vec::new();
    for (usage, field) in input.uses.iter().zip(&names) {
        let consumed = match usage.source {
            Source::Argument(index) => Consumed::Argument(*index),
            Source::Args => Consumed::Rest,
            Source::Property(name) => Consumed::Property(cx.names.resolve(*name)),
            _ => continue,
        };
        sites.push((usage.site, usage.source));
        uses.push((field.as_str(), consumed));
    }
    let claimed: Vec<(Consumed<'_>, &str)> = input
        .keys
        .iter()
        .map(|key| (Consumed::Argument(*key), KEY))
        .collect();
    let found = overlaps(&claimed, &uses);
    let mut indices: BTreeSet<u64> = input.keys.iter().copied().collect();
    let mut rest = false;
    let mut properties = BTreeSet::new();
    for (at, (site, source)) in sites.iter().enumerate() {
        if let Some(overlap) = found.iter().find(|o| o.at == at) {
            cx.diags
                .error(CONFLICTING_BINDING, *site, &overlap.values());
            continue;
        }
        match source {
            Source::Argument(index) => {
                indices.insert(*index);
            }
            Source::Args => rest = true,
            Source::Property(name) => {
                properties.insert(*name);
            }
            _ => {}
        }
    }
    let indexed = indices.last().map_or(0, |last| last.saturating_add(1));
    if indexed > indices.len() as u64 {
        for index in (0..indexed).filter(|i| !indices.contains(i)) {
            let reason =
                format!("argument {index} is not consumed while a later argument is bound");
            cx.diags
                .error(INVALID_BINDING, input.site, &[&input.label, &reason]);
        }
    }
    (Arity { indexed, rest }, properties.into_iter().collect())
}

/// Compile a node declaration's ordered sequence into its scope's state
/// machine. Every step alternative must name a child the scope permits;
/// bounds were normalized when the declaration was collected (absent
/// bounds mean exactly once).
fn compile_sequence(
    cx: &mut Cx<'_>,
    input: &ScopeInput<'_>,
    rules: &ScopeRules,
    sequence: &SequenceDecl,
) -> Option<Vec<SequenceStep>> {
    for step in &sequence.steps {
        if step.alternatives.is_empty() {
            let reason = "a sequence step names no node and no alternatives".to_owned();
            cx.diags
                .error(INVALID_BINDING, sequence.site, &[&input.label, &reason]);
            continue;
        }
        if step.min > step.max {
            let reason = format!(
                "a sequence step has min-occurs {} above max-occurs {}",
                step.min, step.max
            );
            cx.diags
                .error(INVALID_BINDING, sequence.site, &[&input.label, &reason]);
            continue;
        }
        for name in &step.alternatives {
            if !rules.permits(*name) {
                let spelled = cx.names.resolve(*name).to_owned();
                let reason = format!("sequence names {spelled}, which is not a permitted child");
                cx.diags
                    .error(INVALID_BINDING, sequence.site, &[&input.label, &reason]);
            }
        }
    }
    if sequence.steps.is_empty() {
        let reason = "sequence names no step".to_owned();
        cx.diags
            .error(INVALID_BINDING, sequence.site, &[&input.label, &reason]);
        return None;
    }
    Some(sequence.steps.clone())
}
