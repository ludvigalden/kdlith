//! Reading the deferral markers, `context` slots, the `document` rule,
//! and `import` requests.

use crate::compile::Cx;
use crate::compile::reader::Construct;
use crate::deferred::{defers, is_discovery_path};
use kdlith_runtime::Repeated;

use crate::definition::{Deferral, DocumentRule, ImportDecl, SlotDecl};
use crate::diag::{DEFERRED_MECHANISM, Site, duplicate};
use crate::names::Sym;
use crate::syntax::SyntaxNode;

/// Collect a dialect root's `deferred` markers before anything else is
/// read, so every construct is checked against the full deferral set.
pub(crate) fn deferrals(cx: &mut Cx<'_>, doc: usize, root: &SyntaxNode) {
    for child in root
        .children
        .iter()
        .filter(|child| child.name == "deferred")
    {
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let mechanism = part.string_arg(cx, 0, "mechanism");
        let note = part.string_prop(cx, "note");
        part.finish_leaf(cx);
        let Some(mechanism) = mechanism else { continue };
        if !cx.kebab(site, "deferred", &mechanism, "mechanism") {
            continue;
        }
        let Some(note) = note else {
            cx.malformed(site, &format!("deferred {mechanism}"), "missing note");
            continue;
        };
        if let Some(first) = cx
            .deferrals
            .iter()
            .find(|d| d.mechanism == mechanism)
            .map(|d| d.site)
        {
            duplicate(
                &mut cx.diags,
                cx.kind,
                (site, first),
                "deferral",
                &mechanism,
            );
            continue;
        }
        cx.deferrals.push(Deferral {
            mechanism,
            note,
            site,
        });
    }
}

pub(crate) fn context(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    Construct::new(cx, doc, node).finish(cx);
    for child in &node.children {
        if child.name != "slot" {
            cx.unknown(doc, child, "context");
            continue;
        }
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let name = part.string_arg(cx, 0, "slot name");
        let ty = part.string_arg(cx, 1, "slot type");
        let namespace = part.string_prop(cx, "namespace");
        let description = part.string_prop(cx, "description");
        part.finish_leaf(cx);
        let (Some(name), Some(ty)) = (name, ty) else {
            continue;
        };
        let label = format!("slot {name}");
        if !cx.kebab(site, &label, &name, "slot name") {
            continue;
        }
        if let Some(namespace) = &namespace
            && !cx.kebab(site, &label, namespace, "namespace")
        {
            continue;
        }
        let Some(ty) = cx.type_expr(site, &ty) else {
            continue;
        };
        let sym = cx.names.intern(&name);
        if let Some(first) = cx
            .slots
            .iter()
            .find(|slot| slot.name == sym)
            .map(|slot| slot.site)
        {
            duplicate(&mut cx.diags, cx.kind, (site, first), "context slot", &name);
            continue;
        }
        let namespace = namespace.map(|namespace| cx.names.intern(&namespace));
        cx.slots.push(SlotDecl {
            name: sym,
            ty,
            namespace,
            description,
            site,
        });
    }
}

pub(crate) fn rule(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    let site = Site::new(doc, node.span);
    Construct::new(cx, doc, node).finish(cx);
    if let Some(earlier) = cx.document_rule.as_ref().map(|rule| rule.site) {
        duplicate(&mut cx.diags, cx.kind, (site, earlier), "document", "rule");
        return;
    }
    let mut first = None;
    let mut following = Vec::new();
    let mut seen = [None; 2];
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        match child.name.as_str() {
            "first" => {
                if !cx.only_once(&mut seen[0], child_site, "document", "first") {
                    continue;
                }
                Construct::new(cx, doc, child).finish(cx);
                let mut group = None;
                for inner in &child.children {
                    if inner.name != "exactly-one" {
                        cx.unknown(doc, inner, "first");
                        continue;
                    }
                    let inner_site = Site::new(doc, inner.span);
                    if !cx.only_once(&mut group, inner_site, "first", "exactly-one") {
                        continue;
                    }
                    first = Some(name_list(cx, doc, inner, "exactly-one"));
                }
                if group.is_none() {
                    cx.malformed(child_site, "first", "missing exactly-one");
                }
            }
            "following" => {
                if cx.only_once(&mut seen[1], child_site, "document", "following") {
                    following = name_list(cx, doc, child, "following");
                }
            }
            _ => cx.unknown(doc, child, "document"),
        }
    }
    if first.as_ref().is_some_and(Vec::is_empty) {
        cx.malformed(site, "document", "first names no node");
    }
    cx.document_rule = Some(DocumentRule {
        first,
        following,
        site,
    });
}

fn name_list(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode, what: &str) -> Vec<Sym> {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let names = part.rest_strings(cx, 0, "node name");
    part.finish_leaf(cx);
    let mut out: Vec<Sym> = Vec::new();
    for name in names {
        if !cx.kebab(site, what, &name, "node name") {
            continue;
        }
        let sym = cx.names.intern(&name);
        if out.contains(&sym) {
            cx.malformed(site, what, &format!("names {name} twice"));
            continue;
        }
        out.push(sym);
    }
    out
}

pub(crate) fn import(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let path = part.string_arg(cx, 0, "path");
    let repeated = part.string_prop(cx, "repeated");
    part.finish(cx);
    let mut project = Vec::new();
    let mut seen = None;
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        if child.name != "project" {
            cx.unknown(doc, child, "import");
            continue;
        }
        if cx.only_once(&mut seen, child_site, "import", "project") {
            project = name_list(cx, doc, child, "project");
        }
    }
    let repeated = match repeated
        .as_deref()
        .map(|name| (name, Repeated::from_name(name)))
    {
        None => Repeated::Reject,
        Some((_, Some(policy))) => policy,
        Some((other, None)) => {
            cx.malformed(site, "import", &format!("unknown repeated policy {other}"));
            return;
        }
    };
    let Some(path) = path else { return };
    if path.is_empty() {
        cx.malformed(site, "import", "empty path");
        return;
    }
    if is_discovery_path(&path) {
        let construct = format!("import {path:?}");
        if defers(&cx.deferrals, "discovery-grammars") {
            cx.diags.error(
                DEFERRED_MECHANISM,
                site,
                &[&construct, "discovery-grammars"],
            );
        } else {
            cx.malformed(site, &construct, "import paths are literal");
        }
        return;
    }
    cx.imports.push(ImportDecl {
        path,
        project,
        repeated,
        site,
    });
}
