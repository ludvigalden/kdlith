//! Reading `context` slots, the `document` rule, and `import` requests.

use crate::compile::Cx;
use crate::compile::reader::Construct;
use crate::discovery::is_expression;
use kdlith_runtime::Repeated;

use crate::definition::{DocumentRule, ImportDecl, MergeDuplicates, SlotDecl, SlotDuplicates};
use crate::diag::{Site, duplicate};
use crate::names::Sym;
use crate::syntax::SyntaxNode;

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
        // The canonical dialect's own slots declare no accumulation; the
        // reader path lands accumulate with the expanded-surface issue.
        // Nor do they declare a duplicate policy: the seed reads none,
        // so a canonical slot keeps the fail-closed reject default.
        cx.slots.push(SlotDecl {
            name: sym,
            ty,
            namespace,
            description,
            accumulate: None,
            merge_duplicates: MergeDuplicates::Reject,
            duplicates: SlotDuplicates::Reject,
            layers: Vec::new(),
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
                    cx.cardinality(child_site, "exactly-one under first", "0", "at least 1");
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
    let alias = part.string_prop(cx, "as");
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
            cx.not_member(site, "import.repeated", "RepeatedImport", other);
            return;
        }
    };
    let alias = match alias {
        Some(alias) if cx.kebab(site, "import", &alias, "alias") => Some(cx.names.intern(&alias)),
        Some(_) => return,
        None => None,
    };
    let Some(path) = path else { return };
    if path.is_empty() {
        cx.malformed(site, "import", "empty path");
        return;
    }
    if is_expression(&path) {
        // The seed bootstrap compiles only the canonical dialect, which
        // binds no discovery grammar; a schema's grammar-admitted
        // expressions arrive through the reader path, which validates
        // them against the compiled grammar.
        let construct = format!("import {path:?}");
        cx.malformed(site, &construct, "import paths are literal");
        return;
    }
    if let Some(alias) = alias
        && cx.imports.iter().any(|import| import.alias == Some(alias))
    {
        let first = cx
            .imports
            .iter()
            .find(|import| import.alias == Some(alias))
            .map(|import| import.site);
        if let Some(first) = first {
            crate::diag::duplicate_key(
                &mut cx.diags,
                (site, first),
                "the alias of import",
                "document.imports",
            );
        }
        return;
    }
    cx.imports.push(ImportDecl {
        path,
        project,
        repeated,
        alias,
        site,
    });
}
