//! Reading `types` containers into the TypeCatalog.

use crate::compile::Cx;
use crate::compile::expansions::Pending;
use crate::compile::reader::Construct;
use crate::decode::found;
use crate::diag::{NOT_INSTALLED, Site, duplicate, duplicate_key};
use crate::names::Sym;
use crate::syntax::SyntaxNode;
use crate::types::{
    CaptureDef, CodecDecl, Constraint, DefaultSpec, EnumMember, EnumType, FieldDef, FormatRule,
    MapType, ObjectType, Sensitivity, StringType, TypeDecl, TypeExpr, TypeKind, UnionType,
    ValidatorUse, Variant,
};
use kdlith_model::{FieldMerge, Value};
use regex::Regex;

pub(crate) fn container(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) {
    Construct::new(cx, doc, node).finish(cx);
    for child in &node.children {
        let kind = match child.name.as_str() {
            "string" => string(cx, doc, child),
            "enum" => enumeration(cx, doc, child),
            "object" => object(cx, doc, child),
            "union" => union(cx, doc, child),
            "map" => map(cx, doc, child),
            "codec" => codec(cx, doc, child),
            _ => {
                cx.unknown(doc, child, "types");
                continue;
            }
        };
        let Some((name, parameters, kind)) = kind else {
            continue;
        };
        declare(cx, Site::new(doc, child.span), name, parameters, kind);
    }
}

pub(crate) fn declare(
    cx: &mut Cx<'_>,
    site: Site,
    name: String,
    parameters: Vec<Sym>,
    kind: TypeKind,
) {
    let sym = cx.names.intern(&name);
    let decl = TypeDecl {
        name: sym,
        parameters,
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
    let name = name.filter(|name| cx.type_name(construct.site(), &construct.label(), name));
    (construct, name)
}

/// One `parameter T` child of a declaring node: an uppercase identifier
/// whose order is significant.
fn parameter(cx: &mut Cx<'_>, doc: usize, child: &SyntaxNode) -> Option<Sym> {
    let site = Site::new(doc, child.span);
    let mut part = Construct::new(cx, doc, child);
    let name = part.string_arg(cx, 0, "parameter name");
    part.finish_leaf(cx);
    let name = name?;
    if !crate::type_expr::is_type_name(&name) {
        let spelled = found(&Value::String(name));
        cx.mistyped(site, "the name of parameter", "TypeName", &spelled);
        return None;
    }
    Some(cx.names.intern(&name))
}

/// One `include TARGET` child among a declaring node's keyed members,
/// recorded for the expansion pass to splice.
fn include(cx: &mut Cx<'_>, doc: usize, child: &SyntaxNode, owner: &str, at: usize) {
    let site = Site::new(doc, child.span);
    let mut part = Construct::new(cx, doc, child);
    let target = part.string_arg(cx, 0, "include target");
    part.finish_leaf(cx);
    let Some(target) = target else {
        return;
    };
    if !cx.type_name(site, &format!("include of {owner}"), &target) {
        return;
    }
    cx.expansions.push(Pending {
        owner: cx.names.intern(owner),
        target: cx.names.intern(&target),
        site,
        at,
    });
}

/// A declaring node has no keyed member to expand into.
fn no_member(cx: &mut Cx<'_>, doc: usize, child: &SyntaxNode, owner: &str) {
    let site = Site::new(doc, child.span);
    let label = format!("include of {owner}");
    cx.malformed(
        site,
        &label,
        "the declaring type has no keyed member to include",
    );
}

fn string(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, Vec<Sym>, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let name = name?;
    let label = format!("string {name}");
    let mut ty = StringType::default();
    let mut parameters = Vec::new();
    let mut seen = [None; 4];
    for child in &node.children {
        let site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "format" => 0,
            "pattern" => 1,
            "validator" => 2,
            "min-length" => 3,
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
                continue;
            }
            "include" => {
                no_member(cx, doc, child, &label);
                continue;
            }
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
    check_parameters(cx, Site::new(doc, node.span), &label, &parameters);
    Some((name, parameters, TypeKind::String(ty)))
}

/// Parameters are unique; a repeated one is a repeated key of the
/// declaration.
fn check_parameters(cx: &mut Cx<'_>, site: Site, label: &str, parameters: &[Sym]) {
    for (at, sym) in parameters.iter().enumerate() {
        if parameters[..at].contains(sym) {
            let spelled = cx.names.resolve(*sym).to_owned();
            duplicate_key(&mut cx.diags, (site, site), &spelled, label);
        }
    }
}

pub(crate) fn format_rule(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    format: &str,
    width: Option<u64>,
) -> Option<FormatRule> {
    let Some(parsed) = crate::types::StringFormat::from_name(format) else {
        cx.not_member(site, "string.format", "StringFormat", format);
        return None;
    };
    match (parsed, width) {
        (crate::types::StringFormat::ZeroPadded, None) => {
            cx.malformed(site, label, "zero-padded needs a width");
            None
        }
        (crate::types::StringFormat::ZeroPadded, Some(0)) => {
            cx.malformed(site, label, "zero-padded width must be at least 1");
            None
        }
        (crate::types::StringFormat::ZeroPadded, Some(_)) | (_, None) => Some(FormatRule {
            format: parsed,
            width,
        }),
        (_, Some(_)) => {
            cx.malformed(site, label, &format!("format {format} takes no width"));
            None
        }
    }
}

pub(crate) fn validator_use(
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

fn enumeration(
    cx: &mut Cx<'_>,
    doc: usize,
    node: &SyntaxNode,
) -> Option<(String, Vec<Sym>, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let name = name?;
    let label = format!("enum {name}");
    let mut members: Vec<(EnumMember, Site)> = Vec::new();
    let mut parameters = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "member" => {}
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
                continue;
            }
            "include" => {
                include(cx, doc, child, &name, members.len());
                continue;
            }
            _ => {
                cx.unknown(doc, child, "enum");
                continue;
            }
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
    check_parameters(cx, Site::new(doc, node.span), &label, &parameters);
    let members = members.into_iter().map(|(member, _)| member).collect();
    Some((name, parameters, TypeKind::Enum(EnumType { members })))
}

fn object(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, Vec<Sym>, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let name = name?;
    let label = format!("object {name}");
    let mut fields: Vec<(FieldDef, Site)> = Vec::new();
    let mut constraints = Vec::new();
    let mut constraint_block = None;
    let mut parameters = Vec::new();
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
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
            }
            "include" => include(cx, doc, child, &name, fields.len()),
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
    check_parameters(cx, Site::new(doc, node.span), &label, &parameters);
    Some((
        name,
        parameters,
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
    let sensitivity = part.string_prop(cx, "sensitivity");
    let merge = part.string_prop(cx, "merge");
    part.finish(cx);
    let sensitivity = sensitivity.and_then(|value| match Sensitivity::from_name(&value) {
        Some(parsed) => Some(parsed),
        None => {
            cx.not_member(site, "field.sensitivity", "Sensitivity", &value);
            None
        }
    });
    let merge = merge.and_then(|value| match FieldMerge::from_name(&value) {
        Some(parsed) => Some(parsed),
        None => {
            cx.not_member(site, "field.merge", "MergePolicy", &value);
            None
        }
    });
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
        if slot == 0 {
            let mut value = Construct::new(cx, doc, child);
            let literal = value.value_arg(cx, 0, "value");
            value.finish_leaf(cx);
            documented_default = literal;
            continue;
        }
        // A fallback is a literal value or a computed descriptor,
        // discriminated by its `computed` property.
        let mut part = Construct::new(cx, doc, child);
        let value = part.opt_value_arg(0);
        let computed = part.value_prop("computed");
        part.finish(cx);
        let mut dependencies = Vec::new();
        for inner in &child.children {
            let inner_site = Site::new(doc, inner.span);
            if inner.name != "dependency" {
                cx.unknown(doc, inner, "fallback");
                continue;
            }
            let mut dep = Construct::new(cx, doc, inner);
            let spelled = dep.string_arg(cx, 0, "dependency");
            dep.finish_leaf(cx);
            if let Some(spelled) = spelled {
                if dependencies.contains(&spelled) {
                    cx.malformed(inner_site, "fallback", &format!("names {spelled} twice"));
                    continue;
                }
                dependencies.push(spelled);
            }
        }
        fallback = default_spec(cx, child_site, &computed, value, dependencies);
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
        sensitivity,
        merge,
    })
}

/// A field's declared default from its spelling: a literal value or a
/// computed descriptor. A default that is neither, or both, is rejected
/// loudly — the engine never guesses which reading was meant.
fn default_spec(
    cx: &mut Cx<'_>,
    site: Site,
    computed: &Option<Value>,
    value: Option<Value>,
    dependencies: Vec<String>,
) -> Option<DefaultSpec> {
    let computed = match computed {
        None => false,
        Some(Value::Bool(flag)) => *flag,
        Some(other) => {
            cx.mistyped(
                site,
                "the computed property of fallback",
                "Bool",
                &found(other),
            );
            return None;
        }
    };
    match (computed, value) {
        (true, Some(_)) => {
            cx.malformed(
                site,
                "fallback",
                "a computed default carries no literal value",
            );
            None
        }
        (true, None) => Some(DefaultSpec::Computed { dependencies }),
        (false, Some(value)) => Some(DefaultSpec::Literal(value)),
        (false, None) => {
            cx.malformed(site, "fallback", "a literal default carries no value");
            None
        }
    }
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

fn union(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, Vec<Sym>, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let name = name?;
    let label = format!("union {name}");
    let mut variants: Vec<(Variant, Site)> = Vec::new();
    let mut parameters = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "variant" => {}
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
                continue;
            }
            "include" => {
                include(cx, doc, child, &name, variants.len());
                continue;
            }
            _ => {
                cx.unknown(doc, child, "union");
                continue;
            }
        }
        let site = Site::new(doc, child.span);
        let mut part = Construct::new(cx, doc, child);
        let discriminator = part.string_arg(cx, 0, "discriminator");
        let ty = part.string_arg(cx, 1, "variant type");
        let prefix = part.string_prop(cx, "prefix");
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
                prefix,
            },
            site,
        ));
    }
    if !node.children.iter().any(|child| child.name == "variant") {
        let site = Site::new(doc, node.span);
        cx.cardinality(site, "variant under union", "0", "at least 1");
    }
    check_parameters(cx, Site::new(doc, node.span), &label, &parameters);
    let variants = variants.into_iter().map(|(variant, _)| variant).collect();
    Some((name, parameters, TypeKind::Union(UnionType { variants })))
}

fn map(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, Vec<Sym>, TypeKind)> {
    let (construct, name) = header(cx, doc, node);
    construct.finish(cx);
    let name = name?;
    let label = format!("map {name}");
    let mut parts: [Option<TypeExpr>; 2] = [None, None];
    let mut seen = [None; 2];
    let mut parameters = Vec::new();
    for child in &node.children {
        let site = Site::new(doc, child.span);
        let slot = match child.name.as_str() {
            "key-type" => 0,
            "value-type" => 1,
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
                continue;
            }
            "include" => {
                no_member(cx, doc, child, &label);
                continue;
            }
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
    check_parameters(cx, site, &label, &parameters);
    Some((
        name,
        parameters,
        TypeKind::Map(MapType {
            key: key?,
            value: value?,
        }),
    ))
}

/// A `codec NAME output=TYPE` declaration: a whole-string `match`, the
/// `capture NAME TYPE` groups it types (covering the match's named
/// groups exactly), an optional `namespace`, and type parameters.
fn codec(cx: &mut Cx<'_>, doc: usize, node: &SyntaxNode) -> Option<(String, Vec<Sym>, TypeKind)> {
    let site = Site::new(doc, node.span);
    let (mut construct, name) = header(cx, doc, node);
    let output = construct.string_prop(cx, "output");
    construct.finish(cx);
    let name = name?;
    let label = format!("codec {name}");
    let Some(output_text) = output else {
        cx.missing(site, &label, "output");
        return None;
    };
    let output = cx.type_expr(site, &output_text)?;
    let mut pattern = None;
    let mut namespace = None;
    let mut captures: Vec<(CaptureDef, Site)> = Vec::new();
    let mut parameters = Vec::new();
    let mut seen = [None; 2];
    for child in &node.children {
        let child_site = Site::new(doc, child.span);
        match child.name.as_str() {
            "match" => {
                if cx.only_once(&mut seen[0], child_site, &label, "match") {
                    let mut part = Construct::new(cx, doc, child);
                    pattern = part.string_arg(cx, 0, "pattern");
                    part.finish_leaf(cx);
                }
            }
            "namespace" => {
                if cx.only_once(&mut seen[1], child_site, &label, "namespace") {
                    let mut part = Construct::new(cx, doc, child);
                    let spelled = part.string_arg(cx, 0, "namespace");
                    part.finish_leaf(cx);
                    namespace = spelled.and_then(|spelled| {
                        if spelled.is_empty() {
                            let found = found(&Value::String(spelled));
                            cx.mistyped(child_site, "the name of namespace", "MemberName", &found);
                            None
                        } else {
                            Some(cx.names.intern(&spelled))
                        }
                    });
                }
            }
            "capture" => {
                let mut part = Construct::new(cx, doc, child);
                let captured = part.string_arg(cx, 0, "capture name");
                let ty = part.string_arg(cx, 1, "capture type");
                part.finish_leaf(cx);
                let (Some(captured), Some(ty)) = (captured, ty) else {
                    continue;
                };
                if captured.is_empty() {
                    let found = found(&Value::String(captured));
                    cx.mistyped(child_site, "the key of capture", "MemberName", &found);
                    continue;
                }
                let Some(ty) = cx.type_expr(child_site, &ty) else {
                    continue;
                };
                if let Some((_, first)) = captures.iter().find(|(c, _)| c.name == captured) {
                    duplicate_key(
                        &mut cx.diags,
                        (child_site, *first),
                        &captured,
                        "codec.captures",
                    );
                    continue;
                }
                captures.push((CaptureDef { name: captured, ty }, child_site));
            }
            "parameter" => {
                if let Some(sym) = parameter(cx, doc, child) {
                    parameters.push(sym);
                }
            }
            "include" => include(cx, doc, child, &name, captures.len()),
            _ => cx.unknown(doc, child, "codec"),
        }
    }
    let Some(pattern) = pattern else {
        cx.missing(site, &label, "match");
        return None;
    };
    let regex = match Regex::new(&format!("^(?:{pattern})$")) {
        Ok(regex) => regex,
        Err(error) => {
            let reason = format!("match does not compile: {error}");
            cx.malformed(site, &label, &reason);
            return None;
        }
    };
    // The capture set covers the match's named groups exactly.
    let mut groups: Vec<&str> = regex.capture_names().flatten().collect();
    groups.sort_unstable();
    let mut declared: Vec<&str> = captures.iter().map(|(c, _)| c.name.as_str()).collect();
    declared.sort_unstable();
    if groups != declared {
        let missing: Vec<&str> = groups
            .iter()
            .filter(|g| !declared.contains(g))
            .copied()
            .collect();
        let extra: Vec<&str> = declared
            .iter()
            .filter(|d| !groups.contains(d))
            .copied()
            .collect();
        let reason = format!(
            "the captures cover the match's named groups exactly; missing [{}], undeclared [{}]",
            missing.join(", "),
            extra.join(", ")
        );
        cx.malformed(site, &label, &reason);
        return None;
    }
    check_parameters(cx, site, &label, &parameters);
    let captures = captures.into_iter().map(|(capture, _)| capture).collect();
    Some((
        name,
        parameters,
        TypeKind::Codec(CodecDecl {
            output,
            pattern,
            captures,
            namespace,
        }),
    ))
}

/// `^[A-Za-z][A-Za-z0-9_-]*$`, the dialect's `FieldName`.
pub(crate) fn field_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
