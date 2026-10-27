//! Node groups: `group NAME MEMBER...` names node names once; each
//! group reference expands to per-member rules, sources, and parents
//! before scopes are built.

use crate::bindings::{ChildRule, GroupSourceKind, NodeDecl, Placement, Source};
use crate::compile::Cx;
use crate::compile::link::unresolved;
use crate::definition::CheckScope;
use crate::diag::{EMPTY_NODE_GROUP, INVALID_BINDING, Site, duplicate};
use crate::names::Sym;

/// One declared node group: its name and members, in written order.
#[derive(Clone, Debug)]
pub(crate) struct GroupDecl {
    pub(crate) name: Sym,
    pub(crate) members: Vec<Sym>,
    pub(crate) site: Site,
}

/// Record one `group` declaration. A name declared twice is a duplicate
/// declaration; a group without members is refused where it is
/// declared, whether or not anything references it.
pub(crate) fn declare(cx: &mut Cx<'_>, site: Site, name: &str, nodes: &[String]) {
    let sym = cx.names.intern(name);
    if let Some(first) = cx.groups.iter().find(|g| g.name == sym).map(|g| g.site) {
        duplicate(&mut cx.diags, cx.kind, (site, first), "node group", name);
        return;
    }
    if nodes.is_empty() {
        cx.diags.error(EMPTY_NODE_GROUP, site, &[name]);
        return;
    }
    let mut members: Vec<Sym> = Vec::new();
    for member in nodes {
        let member = cx.names.intern(member);
        if members.contains(&member) {
            let reason = format!("names {} twice", cx.names.resolve(member));
            cx.malformed(site, &format!("group {name}"), &reason);
            continue;
        }
        members.push(member);
    }
    cx.groups.push(GroupDecl {
        name: sym,
        members,
        site,
    });
}

/// Check every group's members and expand every group reference.
pub(crate) fn expand(cx: &mut Cx<'_>) {
    let complete = cx.scope == CheckScope::Complete;
    if complete {
        members(cx);
    }
    let mut nodes = std::mem::take(&mut cx.nodes);
    for decl in &mut nodes {
        children(cx, decl, complete);
        parents(cx, decl, complete);
        sources(cx, decl);
    }
    cx.nodes = nodes;
}

/// Every member names a node some declaration declares, as an
/// exactly-one group's members must name permitted children. An unknown
/// member is reported once, here, and drops out of every expansion.
fn members(cx: &mut Cx<'_>) {
    let mut found: Vec<(Site, String, String)> = Vec::new();
    let mut groups = std::mem::take(&mut cx.groups);
    for group in &mut groups {
        let label = format!("group {}", cx.names.resolve(group.name));
        group.members.retain(|member| {
            let known = cx.nodes.iter().any(|decl| decl.name == *member);
            if !known {
                let reason = format!(
                    "member {} names no declared node",
                    cx.names.resolve(*member)
                );
                found.push((group.site, label.clone(), reason));
            }
            known
        });
    }
    cx.groups = groups;
    for (site, label, reason) in found {
        cx.diags.error(INVALID_BINDING, site, &[&label, &reason]);
    }
}

/// The group named `name`, or the reference reported: undeclared under
/// complete checking, left for another contribution under local.
fn resolve(cx: &mut Cx<'_>, site: Site, name: Sym, complete: bool) -> Option<Vec<Sym>> {
    if let Some(group) = cx.groups.iter().find(|g| g.name == name) {
        return Some(group.members.clone());
    }
    if complete {
        let spelled = cx.names.resolve(name).to_owned();
        unresolved(cx, site, "node group", &spelled);
    }
    None
}

/// Each `children-group` rule becomes one children rule per member, with
/// the rule's bounds, after the declaration's literal rules. A name that
/// already has a rule contradicts it.
fn children(cx: &mut Cx<'_>, decl: &mut NodeDecl, complete: bool) {
    let rules = std::mem::take(&mut decl.children_groups);
    let mut origin: Vec<(Sym, Option<Sym>)> =
        decl.children.iter().map(|rule| (rule.name, None)).collect();
    for rule in &rules {
        let Some(members) = resolve(cx, rule.site, rule.name, complete) else {
            continue;
        };
        for member in members {
            if let Some((_, earlier)) = origin.iter().find(|(name, _)| *name == member) {
                let group = cx.names.resolve(rule.name);
                let first = match earlier {
                    None => "directly".to_owned(),
                    Some(other) => format!("via group {}", cx.names.resolve(*other)),
                };
                let reason = format!(
                    "children rule for {} is spelled twice ({first} and via group {group})",
                    cx.names.resolve(member)
                );
                let owner = cx.names.resolve(decl.name).to_owned();
                cx.diags
                    .error(INVALID_BINDING, rule.site, &[&owner, &reason]);
                continue;
            }
            origin.push((member, Some(rule.name)));
            decl.children.push(ChildRule {
                name: member,
                min: rule.min,
                max: rule.max,
                branches: rule.branches.clone(),
                site: rule.site,
            });
        }
    }
    decl.children_groups = rules;
}

/// A `parents group=` reference places the node under every member as
/// well; a parent named both positionally and through the group is
/// listed twice, as any repeated parent is.
fn parents(cx: &mut Cx<'_>, decl: &mut NodeDecl, complete: bool) {
    let Some(group) = decl.parent_group else {
        return;
    };
    let at_root = matches!(decl.placement, Placement::RootAndUnder(_));
    let Some(members) = resolve(cx, decl.site, group, complete) else {
        if at_root && decl.placement.parents().is_empty() {
            decl.placement = Placement::Root;
        }
        return;
    };
    let mut under = decl.placement.parents().to_vec();
    for member in members {
        if under.contains(&member) {
            let label = format!("node {}", cx.names.resolve(decl.name));
            let reason = format!("parent {} listed twice", cx.names.resolve(member));
            cx.malformed(decl.site, &label, &reason);
            continue;
        }
        under.push(member);
    }
    decl.placement = match (at_root, under.is_empty()) {
        (true, true) => Placement::Root,
        (true, false) => Placement::RootAndUnder(under),
        (false, _) => Placement::Under(under),
    };
}

/// Each pending group from-source splices one source per member at
/// the reference's position: children sources for `children-group`,
/// child sources for `child-group`, as if hand-written.
fn sources(cx: &mut Cx<'_>, decl: &mut NodeDecl) {
    let owner = cx.names.resolve(decl.name).to_owned();
    // A group-sourced from-source under a case body is refused: case
    // field bindings compile into per-case plans, and a reference the
    // expansion pass cannot splice would silently bind nothing.
    for case in &mut decl.cases {
        for field in &mut case.fields {
            if field.pending_groups.is_empty() {
                continue;
            }
            let pending = std::mem::take(&mut field.pending_groups);
            for group in &pending {
                let reason = match group.kind {
                    GroupSourceKind::Children => {
                        "a children-group source under a case body is not admitted; spell the members per case"
                    }
                    GroupSourceKind::Child => {
                        "a child-group source under a case body is not admitted; spell the members per case"
                    }
                };
                cx.diags
                    .error(INVALID_BINDING, group.site, &[&owner, reason]);
            }
        }
    }
    for field in &mut decl.fields {
        if field.pending_groups.is_empty() {
            continue;
        }
        let pending = std::mem::take(&mut field.pending_groups);
        // Child names the spliced members must not double: the field's
        // concrete child and children sources, then members an earlier
        // group spliced.
        let mut claimed: Vec<(Sym, Option<Sym>)> = field
            .sources
            .iter()
            .filter_map(|source| match source {
                Source::Child { name, .. } | Source::Children { name, .. } => Some((*name, None)),
                _ => None,
            })
            .collect();
        let mut spliced: Vec<(usize, Vec<Source>)> = Vec::with_capacity(pending.len());
        let mut singular = false;
        for group in &pending {
            singular |= group.kind == GroupSourceKind::Child;
            let Some(members) = resolve(cx, group.site, group.name, true) else {
                continue;
            };
            let mut sources = Vec::with_capacity(members.len());
            for member in members {
                if let Some((_, earlier)) = claimed.iter().find(|(name, _)| *name == member) {
                    let via = cx.names.resolve(group.name);
                    let first = match earlier {
                        None => "directly".to_owned(),
                        Some(other) => format!("via group {}", cx.names.resolve(*other)),
                    };
                    let reason = format!(
                        "source for child {} is spelled twice ({first} and via group {via})",
                        cx.names.resolve(member)
                    );
                    cx.diags
                        .error(INVALID_BINDING, group.site, &[&owner, &reason]);
                    continue;
                }
                claimed.push((member, Some(group.name)));
                sources.push(match group.kind {
                    GroupSourceKind::Children => Source::Children {
                        name: member,
                        sources: Vec::new(),
                        compact: false,
                    },
                    GroupSourceKind::Child => Source::Child {
                        name: member,
                        sources: Vec::new(),
                        compact: false,
                    },
                });
            }
            spliced.push((group.position, sources));
        }
        // Positions are ascending, so splicing from the back keeps every
        // earlier position valid.
        for (position, sources) in spliced.into_iter().rev() {
            field.sources.splice(position..position, sources);
        }
        if !singular {
            continue;
        }
        let alternatives = field
            .sources
            .iter()
            .filter(|s| {
                !matches!(
                    s,
                    Source::Context { .. } | Source::Children { .. } | Source::Expand { .. }
                )
            })
            .count();
        if alternatives > 1 && field.collision.is_none() {
            let name = cx.names.resolve(field.name);
            let reason = format!(
                "field {name} has {alternatives} alternative sources and no collision policy; no implicit precedence exists"
            );
            cx.diags
                .error(INVALID_BINDING, field.site, &[&owner, &reason]);
        }
    }
}
