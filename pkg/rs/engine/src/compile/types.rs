//! Reading `types` containers into the TypeCatalog.

use crate::compile::Cx;
use crate::compile::reader::Construct;
use crate::decode::found;
use crate::deferred::defers;
use crate::diag::{DEFERRED_MECHANISM, NOT_INSTALLED, Site, duplicate, duplicate_key};
use crate::names::Sym;
use crate::syntax::SyntaxNode;
use crate::types::{
    Constraint, EnumMember, EnumType, FieldDef, FormatRule, MapType, ObjectType, StringFormat,
    StringType, TypeDecl, TypeExpr, TypeKind, UnionType, ValidatorUse, Variant,
};
use kdlith_model::Value;

pub(crate) fn container(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    Construct::new(cx, doc, node).finish(cx);
    for child in &node.children {
        let kind = match child.name.as_str() {
            "string" => string(cx, doc, child),
            "enum" => enumeration(cx, doc, child),
            "object" => object(cx, doc, child),
            "union" => union(cx, doc, child),
            "map" => map(cx, doc, child),
            _ => {
                cx.unknown(doc, child, "types");
                continue;
            }
        };
        let Some((name, kind)) = kind else { continue };
        declare(cx, Site::new(doc, child.span), name, kind);
    }
}

fn declare(cx: &mut Cx<'_>, site: Site, name: String, kind: TypeKind) {
    let sym = cx.names.intern(&name);
    let decl = TypeDecl {
        name: sym,
        kind,
        site: Some(site),
    };
    if let Err(earlier) = cx.types.push(decl) {
        match cx.types.get(earlier).and_then(|decl| decl.site) {
            Some(first) => duplicate(&mut cx.diags, cx.kind, (site, first), "type", &name),
            None => cx.malformed(site, &format!("type {name}"), "shadows a built-in scalar"),
        }
    }
}

/// The declaring node's name argument, checked as a type name.
fn header<'n>(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &'n SyntaxNode,
) -> (Construct<'n>, Option<String>) {
    let mut construct = Construct::new(cx, doc, node);
    let name = construct.string_arg(cx, 0, "name");
    let name = name.filter(|name| generic(cx, construct.site(), &node.name, name));
    let name = name.filter(|name| cx.type_name(construct.site(), &construct.label(), name));
    (construct, name)
}

/// A generic declaration `Name<T>` relies on `generic-types`.
fn generic(cx: &mut Cx<'_>, site: Site, kind: &str, name: &str) -> bool {
    if !name.contains('<') || !defers(&cx.deferrals, "generic-types") {
        return true;
    }
    let construct = format!("{kind} {name}");
    cx.diags
        .error(DEFERRED_MECHANISM, site, &[&construct, "generic-types"]);
    false
}

fn string(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let label = format!("string {}", name.as_deref().unwrap_or("?"));
    let mut ty = StringType::default();
    let mut seen = [None; 4];
    for child in &node.children {
        let site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "format" => 0,
            "pattern" => 1,
            "validator" => 2,
            "min-length" => 3,
            _ => {
                cx.unknown(doc, child, "string");
                continue;
            }
        };
        if !cx.only_once(&mut seen[slot], site, &label, &child.name) {
            continue;
        }
        let mut part = Construct::new(cx, doc, child);
        match slot {
            0 => {
                let format = part.string_arg(cx, 0, "format");
                let width = part.u64_prop(cx, "width");
                part.finish_leaf(cx);
                ty.format = format.and_then(|format| format_rule(cx, site, &label, &format, width));
            }
            1 => {
                ty.pattern = part.string_arg(cx, 0, "pattern");
                part.finish_leaf(cx);
            }
            2 => {
                let validator = part.string_arg(cx, 0, "validator name");
                let version = part.string_prop(cx, "version");
                part.finish_leaf(cx);
                ty.validator =
                    validator.and_then(|name| validator_use(cx, site, &label, name, version));
            }
            _ => {
                ty.min_length = part.u64_arg(cx, 0, "minimum length");
                part.finish_leaf(cx);
            }
        }
    }
    Some((name?, TypeKind::String(ty)))
}

fn format_rule(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    format: &str,
    width: Option<u64>,
) -> Option<FormatRule> {
    let Some(parsed) = StringFormat::from_name(format) else {
        cx.not_member(site, "string.format", "StringFormat", format);
        return None;
    };
    match (parsed, width) {
        (StringFormat::ZeroPadded, None) => {
            cx.malformed(site, label, "zero-padded needs a width");
            None
        }
        (StringFormat::ZeroPadded, Some(0)) => {
            cx.malformed(site, label, "zero-padded width must be at least 1");
            None
        }
        (StringFormat::ZeroPadded, Some(_)) | (_, None) => Some(FormatRule {
            format: parsed,
            width,
        }),
        (_, Some(_)) => {
            cx.malformed(site, label, &format!("format {format} takes no width"));
            None
        }
    }
}

fn validator_use(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    name: String,
    version: Option<String>,
) -> Option<ValidatorUse> {
    if !cx.kebab(site, label, &name, "validator name") {
        return None;
    }
    if !cx.extensions.has_validator(&name, version.as_deref()) {
        let wanted = match &version {
            Some(version) => format!("{name} version {version}"),
            None => name.clone(),
        };
        cx.diags.error(NOT_INSTALLED, site, &["validator", &wanted]);
        return None;
    }
    Some(ValidatorUse { name, version })
}

fn enumeration(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let mut members: Vec<(EnumMember, Site)> = Vec::new();
    for child in &node.children {
        if child.name != "member" {
            cx.unknown(doc, child, "enum");
            continue;
        }
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let member = part.string_arg(cx, 0, "member name");
        let description = part.string_prop(cx, "description");
        part.finish_leaf(cx);
        let Some(member) = member else { continue };
        if member.is_empty() {
            let empty = found(&Value::String(member));
            cx.mistyped(site, "the key of member", "MemberName", &empty);
            continue;
        }
        if let Some((_, first)) = members.iter().find(|(m, _)| m.name == member) {
            duplicate_key(&mut cx.diags, (site, *first), &member, "enum.members");
            continue;
        }
        members.push((
            EnumMember {
                name: member,
                description,
            },
            site,
        ));
    }
    if !node.children.iter().any(|child| child.name == "member") {
        let site = Site::new(doc, node.span);
        cx.cardinality(site, "member under enum", "0", "at least 1");
    }
    let members = members.into_iter().map(|(member, _)| member).collect();
    Some((name?, TypeKind::Enum(EnumType { members })))
}

fn object(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let label = format!("object {}", name.as_deref().unwrap_or("?"));
    let mut fields: Vec<(FieldDef, Site)> = Vec::new();
    let mut constraints = Vec::new();
    let mut constraint_block = None;
    for child in &node.children {
        let site = Site::new(doc, child.span);
        match child.name.as_str() {
            "field" => {
                let Some(field) = field_def(cx, doc, child) else {
                    continue;
                };
                if let Some((_, first)) = fields.iter().find(|(f, _)| f.name == field.name) {
                    let spelled = cx.names.resolve(field.name).to_owned();
                    duplicate_key(&mut cx.diags, (site, *first), &spelled, "object.fields");
                    continue;
                }
                fields.push((field, site));
            }
            "constraints" => {
                if cx.only_once(&mut constraint_block, site, &label, "constraints") {
                    constraint_list(cx, doc, child, &mut constraints);
                }
            }
            _ => cx.unknown(doc, child, "object"),
        }
    }
    let fields: Vec<FieldDef> = fields.into_iter().map(|(field, _)| field).collect();
    if let Some(site) = constraint_block {
        let known = |sym: &Sym| fields.iter().any(|f| f.name == *sym);
        for constraint in &constraints {
            let named: Vec<Sym> = match constraint {
                Constraint::AllOrNone(list) => list.clone(),
                Constraint::Requires { field, required } => vec![*field, *required],
            };
            for sym in named.iter().filter(|sym| !known(sym)) {
                let spelled = cx.names.resolve(*sym).to_owned();
                cx.malformed(
                    site,
                    &label,
                    &format!("constraint names undeclared field {spelled}"),
                );
            }
        }
    }
    Some((
        name?,
        TypeKind::Object(ObjectType {
            fields,
            constraints,
        }),
    ))
}

fn field_def(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<FieldDef> {
    let site = Site::new(doc, node.span);
    let mut part = Construct::new(cx, doc, node);
    let name = part.string_arg(cx, 0, "field name");
    let ty = part.string_arg(cx, 1, "field type");
    let description = part.string_prop(cx, "description");
    part.finish(cx);
    let label = format!("field {}", name.as_deref().unwrap_or("?"));
    let mut documented_default = None;
    let mut fallback = None;
    let mut seen = [None; 2];
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "documented-default" => 0,
            "fallback" => 1,
            _ => {
                cx.unknown(doc, child, "field");
                continue;
            }
        };
        if !cx.only_once(&mut seen[slot], child_site, &label, &child.name) {
            continue;
        }
        let mut value = Construct::new(cx, doc, child);
        let literal = value.value_arg(cx, 0, "value");
        value.finish_leaf(cx);
        if slot == 0 {
            documented_default = literal;
        } else {
            fallback = literal;
        }
    }
    let name = name?;
    if !field_name(&name) {
        let spelled = found(&Value::String(name));
        cx.mistyped(site, "the key of field", "FieldName", &spelled);
        return None;
    }
    let ty = cx.type_expr(site, &ty?)?;
    Some(FieldDef {
        name: cx.names.intern(&name),
        ty,
        description,
        documented_default,
        fallback,
    })
}

fn constraint_list(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode, out: &mut Vec<Constraint>) {
    Construct::new(cx, doc, node).finish(cx);
    for child in &node.children {
        let mut part = Construct::new(cx, doc, child);
        match child.name.as_str() {
            "all-or-none" => {
                let names = part.rest_strings(cx, 0, "field name");
                part.finish_leaf(cx);
                if names.len() < 2 {
                    cx.malformed(
                        Site::new(doc, child.span),
                        "all-or-none",
                        "names at least two fields",
                    );
                    continue;
                }
                out.push(Constraint::AllOrNone(
                    names.iter().map(|n| cx.names.intern(n)).collect(),
                ));
            }
            "requires" => {
                let field = part.string_arg(cx, 0, "field");
                let required = part.string_arg(cx, 1, "required field");
                part.finish_leaf(cx);
                if let (Some(field), Some(required)) = (field, required) {
                    out.push(Constraint::Requires {
                        field: cx.names.intern(&field),
                        required: cx.names.intern(&required),
                    });
                }
            }
            _ => cx.unknown(doc, child, "constraints"),
        }
    }
}

fn union(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let label = format!("union {}", name.as_deref().unwrap_or("?"));
    let mut variants: Vec<(Variant, Site)> = Vec::new();
    for child in &node.children {
        if child.name != "variant" {
            cx.unknown(doc, child, "union");
            continue;
        }
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let discriminator = part.string_arg(cx, 0, "discriminator");
        let ty = part.string_arg(cx, 1, "variant type");
        part.finish_leaf(cx);
        let (Some(discriminator), Some(ty)) = (discriminator, ty) else {
            continue;
        };
        if !cx.kebab(site, &label, &discriminator, "discriminator") {
            continue;
        }
        let Some(ty) = cx.type_expr(site, &ty) else {
            continue;
        };
        let sym = cx.names.intern(&discriminator);
        if let Some((_, first)) = variants.iter().find(|(v, _)| v.discriminator == sym) {
            duplicate_key(
                &mut cx.diags,
                (site, *first),
                &discriminator,
                "union.variants",
            );
            continue;
        }
        variants.push((
            Variant {
                discriminator: sym,
                ty,
            },
            site,
        ));
    }
    if !node.children.iter().any(|child| child.name == "variant") {
        let site = Site::new(doc, node.span);
        cx.cardinality(site, "variant under union", "0", "at least 1");
    }
    let variants = variants.into_iter().map(|(variant, _)| variant).collect();
    Some((name?, TypeKind::Union(UnionType { variants })))
}

fn map(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let label = format!("map {}", name.as_deref().unwrap_or("?"));
    let mut parts: [Option<TypeExpr>; 2] = [None, None];
    let mut seen = [None; 2];
    for child in &node.children {
        let site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "key-type" => 0,
            "value-type" => 1,
            _ => {
                cx.unknown(doc, child, "map");
                continue;
            }
        };
        if !cx.only_once(&mut seen[slot], site, &label, &child.name) {
            continue;
        }
        let mut part = Construct::new(cx, doc, child);
        let ty = part.string_arg(cx, 0, "type");
        part.finish_leaf(cx);
        parts[slot] = ty.and_then(|ty| cx.type_expr(site, &ty));
    }
    let [key, value] = parts;
    let site = Site::new(doc, node.span);
    for (at, child) in ["key-type", "value-type"].into_iter().enumerate() {
        if seen[at].is_none() {
            // As binding reports it: the child is below its min-occurs,
            // and the field it supplies is missing.
            let what = format!("{child} under map");
            cx.cardinality(site, &what, "0", "at least 1");
            cx.missing(site, "map", child);
        }
    }
    Some((
        name?,
        TypeKind::Map(MapType {
            key: key?,
            value: value?,
        }),
    ))
}

/// `^[A-Za-z][A-Za-z0-9_-]*$`, the dialect's `FieldName`.
pub(crate) fn field_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
