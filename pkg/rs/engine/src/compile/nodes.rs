//! Reading `node` declarations: roles, placement, field bindings and
//! their sources, child rules, effect groups, and exactly-one groups.

use crate::bindings::{
    ChildRule, Collision, ContextPolicy, DuplicateKeys, Effect, EffectGroup, EffectTarget,
    FieldBinding, NodeDecl, Placement, Role, SetFrom, Source,
};
use crate::compile::Cx;
use crate::compile::reader::Construct;
use crate::compile::types::field_name;
use crate::deferred::defers;
use crate::diag::{DEFERRED_MECHANISM, INVALID_BINDING, NOT_INSTALLED, Site, duplicate_key};
use crate::names::Sym;
use crate::syntax::{SyntaxNode, parse_u64};

/// Nested `child` and `children` sources nest at most this deep.
const MAX_SOURCE_DEPTH: usize = 16;

pub(crate) fn declaration(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    let site = Site::new(doc, node.span);
    let mut construct = Construct::new(cx, doc, node);
    let name = construct.string_arg(cx, 0, "name");
    let role = construct.string_prop(cx, "role");
    let ty = construct.string_prop(cx, "type");
    let parent = construct.string_prop(cx, "parent");
    construct.finish(cx);
    let Some(name) = name else { return };
    let label = format!("node {name}");
    if !cx.kebab(site, &label, &name, "node name") {
        return;
    }
    let role = role.and_then(|role| match Role::from_name(&role) {
        Some(parsed) => Some(parsed),
        None => {
            cx.not_member(site, "node.role", "Role", &role);
            None
        }
    });
    let typed = ty.is_some();
    let ty = ty
        .filter(|ty| cx.type_name(site, &label, ty))
        .map(|ty| cx.names.intern(&ty));

    let mut parents: Option<Vec<String>> = None;
    let mut parents_site = None;
    let mut fields: Vec<FieldBinding> = Vec::new();
    let mut children: Vec<ChildRule> = Vec::new();
    let mut effects: Vec<EffectGroup> = Vec::new();
    let mut groups: Vec<Vec<Sym>> = Vec::new();
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        match child.name.as_str() {
            "parents" => {
                if cx.only_once(&mut parents_site, child_site, &label, "parents") {
                    let mut part = Construct::new(cx, doc, child);
                    parents = Some(part.rest_strings(cx, 0, "parent name"));
                    part.finish_leaf(cx);
                }
            }
            "field" => {
                if let Some(field) = field_binding(cx, doc, child, &label, typed) {
                    if let Some(earlier) = fields.iter().find(|f| f.name == field.name) {
                        let spelled = cx.names.resolve(field.name).to_owned();
                        let sites = (child_site, earlier.site);
                        duplicate_key(&mut cx.diags, sites, &spelled, "node.fields");
                        continue;
                    }
                    fields.push(field);
                }
            }
            "children" => {
                if let Some(rule) = child_rule(cx, doc, child, &label) {
                    if let Some(earlier) = children.iter().find(|r| r.name == rule.name) {
                        let spelled = cx.names.resolve(rule.name).to_owned();
                        let sites = (child_site, earlier.site);
                        duplicate_key(&mut cx.diags, sites, &spelled, "node.children");
                        continue;
                    }
                    children.push(rule);
                }
            }
            "effects" => {
                if let Some(group) = effect_group(cx, doc, child) {
                    if effects.iter().any(|g| g.target == group.target) {
                        cx.diags.error(
                            INVALID_BINDING,
                            child_site,
                            &[&name, "two effect groups share one target"],
                        );
                        continue;
                    }
                    effects.push(group);
                }
            }
            "exactly-one" => {
                let mut part = Construct::new(cx, doc, child);
                let members = part.rest_strings(cx, 0, "child name");
                part.finish_leaf(cx);
                if members.len() < 2 {
                    cx.malformed(
                        child_site,
                        &label,
                        "exactly-one names at least two children",
                    );
                    continue;
                }
                let members: Vec<Sym> = members.iter().map(|m| cx.names.intern(m)).collect();
                if (1..members.len()).any(|at| members[..at].contains(&members[at])) {
                    cx.malformed(child_site, &label, "exactly-one names a child twice");
                    continue;
                }
                groups.push(members);
            }
            _ => cx.unknown(doc, child, "node"),
        }
    }
    if !effects.is_empty() && role != Some(Role::ContextControl) {
        cx.diags.error(
            INVALID_BINDING,
            site,
            &[&name, "only a context-control node declares effects"],
        );
    }
    let placement = match (parent, parents) {
        (Some(_), Some(_)) => {
            cx.diags.error(
                INVALID_BINDING,
                site,
                &[&name, "gives both parent= and parents"],
            );
            return;
        }
        (Some(parent), None) => vec![parent],
        (None, Some(parents)) if parents.is_empty() => {
            cx.malformed(site, &label, "parents names no parent");
            return;
        }
        (None, Some(parents)) => parents,
        (None, None) => Vec::new(),
    };
    let mut under = Vec::new();
    for parent in placement {
        if !cx.kebab(site, &label, &parent, "parent name") {
            return;
        }
        let sym = cx.names.intern(&parent);
        if under.contains(&sym) {
            cx.malformed(site, &label, &format!("parent {parent} listed twice"));
            return;
        }
        under.push(sym);
    }
    if typed && ty.is_none() {
        // The rejected type= is reported; its fields are not also
        // reported as untyped.
        return;
    }
    cx.nodes.push(NodeDecl {
        name: cx.names.intern(&name),
        role,
        ty,
        placement: if under.is_empty() {
            Placement::Root
        } else {
            Placement::Under(under)
        },
        fields,
        children,
        effects,
        groups,
        site,
    });
}

fn field_binding(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &SyntaxNode,
    owner: &str,
    typed_owner: bool,
) -> Option<FieldBinding> {
    let site = Site::new(doc, node.span);
    let mark = cx.diags.count();
    let mut part = Construct::new(cx, doc, node);
    let name = part.string_arg(cx, 0, "field name");
    let inline = part.opt_string_arg(cx, 1, "field type");
    let arg = part.u64_prop(cx, "arg");
    let property = part.string_prop(cx, "property");
    let child = part.string_prop(cx, "child");
    let context = part.string_prop(cx, "context");
    part.finish(cx);
    let name = name?;
    let label = format!("field {name} of {owner}");
    if !field_name(&name) {
        cx.malformed(
            site,
            &label,
            "field names start with a letter and hold letters, digits, '_' and '-'",
        );
        return None;
    }
    let ty = match inline {
        Some(text) if typed_owner => {
            let reason = format!("field {name} gives a type although the node's type= declares it");
            let _ = text;
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
            return None;
        }
        Some(text) => Some(cx.type_expr(site, &text)?),
        None if typed_owner => None,
        None => {
            let reason = format!("field {name} needs a type because the node has no type=");
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
            return None;
        }
    };
    let mut sources = Vec::new();
    if let Some(index) = arg {
        sources.push(Source::Argument(index));
    }
    if let Some(property) = property {
        sources.push(Source::Property(cx.names.intern(&property)));
    }
    if let Some(child) = child {
        if cx.kebab(site, &label, &child, "child name") {
            sources.push(Source::Child {
                name: cx.names.intern(&child),
                sources: Vec::new(),
            });
        }
    }
    if let Some(slot) = context {
        sources.push(Source::Context {
            slot: cx.names.intern(&slot),
            policy: None,
        });
    }
    let mut collision = None;
    let mut duplicate_keys = None;
    let mut seen = [None; 3];
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "from" => 0,
            "collision" => 1,
            "duplicate-keys" => 2,
            _ => {
                cx.unknown(doc, child, "field");
                continue;
            }
        };
        if !cx.only_once(&mut seen[slot], child_site, &label, &child.name) {
            continue;
        }
        if slot == 0 {
            Construct::new(cx, doc, child).finish(cx);
            sources.extend(source_list(cx, doc, child, true, 0));
            continue;
        }
        let mut policy = Construct::new(cx, doc, child);
        let value = policy.string_arg(cx, 0, "policy");
        policy.finish_leaf(cx);
        match value.as_deref() {
            Some("reject") if slot == 1 => collision = Some(Collision::Reject),
            Some("reject") => duplicate_keys = Some(DuplicateKeys::Reject),
            Some(other) => {
                let (what, ty) = if slot == 1 {
                    ("field.collision", "Collision")
                } else {
                    ("field.duplicate-keys", "DuplicateKeys")
                };
                cx.not_member(child_site, what, ty, other);
            }
            None => {}
        }
    }
    if sources.is_empty() {
        // A source rejected while reading is reported already; the field
        // is not also reported as having none.
        if cx.diags.count() == mark {
            let reason = format!("field {name} has no source");
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        }
        return None;
    }
    let spellings = sources
        .iter()
        .filter(|s| !matches!(s, Source::Context { .. }))
        .count();
    let alternatives = sources
        .iter()
        .filter(|s| !matches!(s, Source::Context { .. } | Source::Children { .. }))
        .count();
    if alternatives > 1 && collision.is_none() {
        let reason = format!(
            "field {name} has {alternatives} alternative sources and no collision policy; no implicit precedence exists"
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    if duplicate_keys.is_some() && !sources.iter().any(|s| matches!(s, Source::Entries { .. })) {
        let reason = format!("field {name} sets duplicate-keys without a keyed entries source");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    let explicit = spellings > 0;
    let unpoliced = sources
        .iter()
        .any(|s| matches!(s, Source::Context { policy: None, .. }));
    if explicit && unpoliced {
        let reason = format!(
            "field {name} combines an explicit source with a context source that names no policy"
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    Some(FieldBinding {
        name: cx.names.intern(&name),
        ty,
        sources,
        collision,
        duplicate_keys,
        site,
    })
}

/// The sources listed in a `from` block (`top`) or inside a nested
/// `child`/`children` source, which admits only the positional, property,
/// and child sources.
fn source_list(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &SyntaxNode,
    top: bool,
    depth: usize,
) -> Vec<Source> {
    let mut sources = Vec::new();
    if depth > MAX_SOURCE_DEPTH {
        cx.malformed(
            Site::new(doc, node.span),
            &node.name,
            &format!("sources nest deeper than {MAX_SOURCE_DEPTH}"),
        );
        return sources;
    }
    for child in &node.children {
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let label = part.label();
        let source = match child.name.as_str() {
            "argument" => part.u64_arg(cx, 0, "index").map(Source::Argument),
            "args" => Some(Source::Args),
            "property" => part
                .string_arg(cx, 0, "property name")
                .map(|p| Source::Property(cx.names.intern(&p))),
            "child" | "children" => {
                let name = part.string_arg(cx, 0, "child name");
                part.finish(cx);
                let nested = source_list(cx, doc, child, false, depth + 1);
                let Some(name) = name.filter(|n| cx.kebab(site, &label, n, "child name")) else {
                    continue;
                };
                let name = cx.names.intern(&name);
                sources.push(if child.name == "child" {
                    Source::Child {
                        name,
                        sources: nested,
                    }
                } else {
                    Source::Children {
                        name,
                        sources: nested,
                    }
                });
                continue;
            }
            "entries" if top => {
                let node_name = part.string_arg(cx, 0, "entry node");
                let key = part.string_prop(cx, "key");
                let value = part.string_prop(cx, "value");
                entries(cx, site, &label, node_name, key, value)
            }
            "context" if top => {
                let slot = part.string_arg(cx, 0, "slot");
                let policy = part.string_prop(cx, "policy");
                let policy = match policy.as_deref() {
                    None => Some(None),
                    Some("fallback") => Some(Some(ContextPolicy::Fallback)),
                    Some("fixed") => Some(Some(ContextPolicy::Fixed)),
                    Some(other @ ("accumulate" | "accumulating"))
                        if defers(&cx.deferrals, "accumulating-context") =>
                    {
                        let construct = format!("{label} policy {other}");
                        cx.diags.error(
                            DEFERRED_MECHANISM,
                            site,
                            &[&construct, "accumulating-context"],
                        );
                        None
                    }
                    Some(other) => {
                        cx.not_member(site, "context.policy", "ContextPolicy", other);
                        None
                    }
                };
                match (slot, policy) {
                    (Some(slot), Some(policy)) => Some(Source::Context {
                        slot: cx.names.intern(&slot),
                        policy,
                    }),
                    _ => None,
                }
            }
            _ => {
                cx.unknown(doc, child, &node.name);
                continue;
            }
        };
        part.finish_leaf(cx);
        sources.extend(source);
    }
    sources
}

fn entries(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    node: Option<String>,
    key: Option<String>,
    value: Option<String>,
) -> Option<Source> {
    let Some(key) = key else {
        cx.missing(site, "entries", "key");
        return None;
    };
    let index = key
        .strip_prefix("arg(")
        .and_then(|rest| rest.strip_suffix(')'))
        .and_then(parse_u64);
    let Some(index) = index else {
        cx.malformed(site, label, &format!("key {key:?} is not arg(N)"));
        return None;
    };
    let Some(value) = value else {
        cx.missing(site, "entries", "value");
        return None;
    };
    let node = node.filter(|node| cx.kebab(site, label, node, "entry node"))?;
    if !cx.type_name(site, label, &value) {
        return None;
    }
    Some(Source::Entries {
        node: cx.names.intern(&node),
        key: index,
        value: cx.names.intern(&value),
    })
}

fn child_rule(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode, owner: &str) -> Option<ChildRule> {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let name = part.string_arg(cx, 0, "child name");
    let min = part.u64_prop(cx, "min-occurs");
    let max = part.u64_prop(cx, "max-occurs");
    part.finish_leaf(cx);
    let name = name.filter(|name| cx.kebab(site, owner, name, "child name"))?;
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        let reason = format!("children {name} has min-occurs {min} above max-occurs {max}");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    if max == Some(0) {
        let reason = format!("children {name} permits no occurrence");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    Some(ChildRule {
        name: cx.names.intern(&name),
        min,
        max,
        site,
    })
}

fn effect_group(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<EffectGroup> {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let target = part.string_prop(cx, "target");
    part.finish(cx);
    let target = match target.as_deref() {
        Some("following") => EffectTarget::Following,
        Some("children") => EffectTarget::Children,
        Some(other) => {
            cx.not_member(site, "effects.target", "EffectTarget", other);
            return None;
        }
        None => {
            cx.missing(site, "effects", "target");
            return None;
        }
    };
    let mut effects = Vec::new();
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let label = part.label();
        let effect = match child.name.as_str() {
            "set" => {
                let slot = part.string_arg(cx, 0, "slot");
                let field = part.string_prop(cx, "field");
                let value = part.value_prop("value");
                let from = match (field, value) {
                    (Some(field), None) => Some(SetFrom::Field(cx.names.intern(&field))),
                    (None, Some(value)) => Some(SetFrom::Value(value)),
                    _ => {
                        cx.malformed(
                            child_site,
                            &label,
                            "set takes exactly one of field= and value=",
                        );
                        None
                    }
                };
                match (slot, from) {
                    (Some(slot), Some(from)) => Some(Effect::Set {
                        slot: cx.names.intern(&slot),
                        from,
                    }),
                    _ => None,
                }
            }
            "remove" => part.string_arg(cx, 0, "slot").map(|slot| Effect::Remove {
                slot: cx.names.intern(&slot),
            }),
            "clear" => part
                .string_arg(cx, 0, "namespace")
                .map(|namespace| Effect::Clear {
                    namespace: cx.names.intern(&namespace),
                }),
            "handler" => {
                let name = part.string_arg(cx, 0, "handler");
                name.filter(|name| {
                    if cx.extensions.has_handler(name) {
                        return true;
                    }
                    cx.diags
                        .error(NOT_INSTALLED, child_site, &["handler", name]);
                    false
                })
                .map(|name| Effect::Handler { name })
            }
            _ => {
                cx.unknown(doc, child, "effects");
                continue;
            }
        };
        part.finish_leaf(cx);
        effects.extend(effect);
    }
    Some(EffectGroup {
        target,
        effects,
        site,
    })
}
