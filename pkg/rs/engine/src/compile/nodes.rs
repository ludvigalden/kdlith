//! Reading `node` declarations: roles, placement, field bindings and
//! their sources, child rules, effect groups, and exactly-one groups.

use crate::bindings::{
    BindingCodec, CaseRule, ChildRule, Collision, ContextPolicy, DependentBinding, DuplicateKeys,
    Effect, EffectGroup, EffectTarget, FieldBinding, NodeDecl, Placement, Role, SetFrom, Source,
};
use crate::compile::Cx;
use crate::compile::reader::Construct;
use crate::compile::types::field_name;
use crate::deferred::defers;
use crate::diag::{DEFERRED_MECHANISM, INVALID_BINDING, NOT_INSTALLED, Site, duplicate_key};
use crate::names::Sym;
use crate::syntax::{SyntaxNode, parse_u64};
use crate::types::TypeExpr;

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
    let mut value = None;
    let mut value_site = None;
    let mut cases: Vec<CaseRule> = Vec::new();
    let mut otherwise = None;
    let mut cases_site = None;
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
            "value" => {
                if cx.only_once(&mut value_site, child_site, &label, "value") {
                    value = value_block(cx, doc, child, &name);
                }
            }
            "cases" => {
                if cx.only_once(&mut cases_site, child_site, &label, "cases") {
                    cases_block(cx, doc, child, &name, &mut cases, &mut otherwise);
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
        value,
        cases,
        otherwise,
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
        .filter(|s| !matches!(s, Source::Context { .. } | Source::Expand { .. }))
        .count();
    let alternatives = sources
        .iter()
        .filter(|s| {
            !matches!(
                s,
                Source::Context { .. } | Source::Children { .. } | Source::Expand { .. }
            )
        })
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
            "expand" if top => {
                let target = part.string_prop(cx, "target");
                let member = part.string_prop(cx, "member");
                expand_source(cx, site, &label, target, member)
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

/// An `expand target=TYPE member=NAME` source: the entries of the named
/// keyed member of the named declaration splice into the collection
/// being bound.
fn expand_source(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    target: Option<String>,
    member: Option<String>,
) -> Option<Source> {
    let missing = if target.is_none() {
        "target"
    } else if member.is_none() {
        "member"
    } else {
        ""
    };
    if !missing.is_empty() {
        cx.missing(site, "expand", missing);
        return None;
    }
    let (target, member) = (target?, member?);
    if !cx.type_name(site, label, &target) {
        return None;
    }
    if member.is_empty() {
        let found = format!("string {member:?}");
        cx.mistyped(site, "the member of expand", "MemberName", &found);
        return None;
    }
    Some(Source::Expand {
        target: cx.names.intern(&target),
        member: cx.names.intern(&member),
    })
}

/// A `value` block: the dependent payload binding of a node declaration.
fn value_block(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &SyntaxNode,
    owner: &str,
) -> Option<DependentBinding> {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let codec = part.string_prop(cx, "codec");
    part.finish(cx);
    let codec = codec.and_then(|name| match BindingCodec::from_name(&name) {
        Some(parsed) => Some(parsed),
        None => {
            cx.not_member(site, "value.codec", "BindingCodec", &name);
            None
        }
    });
    let codec = codec.filter(|codec| {
        if cx.extensions.has_binding_codec(codec.name()) {
            return true;
        }
        cx.diags
            .error(NOT_INSTALLED, site, &["binding codec", codec.name()]);
        false
    });
    let mut against: Option<(String, String)> = None;
    let mut seen = None;
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        if child.name != "against" {
            cx.unknown(doc, child, "value");
            continue;
        }
        if !cx.only_once(&mut seen, child_site, owner, "against") {
            continue;
        }
        let mut part = Construct::new(cx, doc, child);
        let target = part.string_prop(cx, "target");
        let member = part.string_prop(cx, "member");
        part.finish_leaf(cx);
        against = match (target, member) {
            (Some(target), Some(member)) => Some((target, member)),
            _ => {
                cx.missing(child_site, "against", "target and member");
                None
            }
        };
    }
    let codec = codec?;
    let Some((target, member)) = against else {
        cx.missing(site, owner, "against");
        return None;
    };
    for (spelled, what) in [(&target, "target field"), (&member, "member field")] {
        if !crate::compile::types::field_name(spelled) {
            let found = format!("string {spelled:?}");
            cx.mistyped(site, what, "FieldName", &found);
            return None;
        }
    }
    Some(DependentBinding {
        target: cx.names.intern(&target),
        member: cx.names.intern(&member),
        codec,
        site,
    })
}

/// A `cases` block: the property-value dispatch that selects the type a
/// node binds as, top-down, first match winning.
fn cases_block(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &SyntaxNode,
    owner: &str,
    cases: &mut Vec<CaseRule>,
    otherwise: &mut Option<TypeExpr>,
) {
    let site = Site::new(doc, node.span);
    Construct::new(cx, doc, node).finish(cx);
    let mut seen_otherwise = None;
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        match child.name.as_str() {
            "case" => {
                let mut part = Construct::new(cx, doc, child);
                let property = part.string_prop(cx, "property");
                let equals = part.value_prop("equals");
                let ty = part.string_prop(cx, "type");
                part.finish_leaf(cx);
                if property.is_none() {
                    cx.missing(child_site, "case", "property");
                    continue;
                }
                if equals.is_none() {
                    cx.missing(child_site, "case", "equals");
                    continue;
                }
                if ty.is_none() {
                    cx.missing(child_site, "case", "type");
                    continue;
                }
                let Some((property, equals, ty)) =
                    property.zip(equals).zip(ty).map(|((p, e), t)| (p, e, t))
                else {
                    continue;
                };
                if !crate::compile::types::field_name(&property) {
                    let found = format!("string {property:?}");
                    cx.mistyped(child_site, "the property of case", "FieldName", &found);
                    continue;
                }
                let Some(ty) = cx.type_expr(child_site, &ty) else {
                    continue;
                };
                cases.push(CaseRule {
                    property: cx.names.intern(&property),
                    equals,
                    ty,
                    site: child_site,
                });
            }
            "otherwise" => {
                if cx.only_once(&mut seen_otherwise, child_site, owner, "otherwise") {
                    let mut part = Construct::new(cx, doc, child);
                    let ty = part.string_prop(cx, "type");
                    part.finish_leaf(cx);
                    match ty {
                        Some(ty) => *otherwise = cx.type_expr(child_site, &ty),
                        None => cx.missing(child_site, "otherwise", "type"),
                    }
                }
            }
            _ => cx.unknown(doc, child, "cases"),
        }
    }
    if cases.is_empty() {
        cx.cardinality(site, "case under cases", "0", "at least 1");
    }
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
