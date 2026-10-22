//! Conformance: a typed processor's declarations are checked against the
//! compiled definition before it reads anything, in both directions, so
//! the processor can neither miss nor invent a type, node, or slot.

use std::fmt;

use crate::bindings::{NodeDecl, Role};
use crate::definition::{Definition, ReferenceDecl};
use crate::names::Names;
use crate::summary::expr;
use crate::typed::shape::{Declarations, NodeShape, TypeShape};
use crate::types::{TypeDecl, TypeExpr, TypeKind};

/// One point where a processor's declarations and its definition differ.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Disagreement {
    pub subject: String,
    pub reason: String,
}

impl Disagreement {
    fn new(subject: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.subject, self.reason)
    }
}

/// Check `declarations` against `def`; every disagreement is reported.
pub(crate) fn run<T>(def: &Definition, declarations: &Declarations<T>) -> Vec<Disagreement> {
    let mut found = Vec::new();
    root(&mut found, def, declarations);
    types(&mut found, def, declarations);
    nodes(&mut found, def, declarations);
    slots(&mut found, def, declarations);
    handlers(&mut found, def, declarations);
    found
}

fn root<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let theirs = format!(
        "{} {} version {}",
        def.kind.root_name(),
        def.name,
        def.version.as_deref().unwrap_or("-")
    );
    let ours = format!(
        "{} {} version {}",
        declarations.kind.root_name(),
        declarations.name,
        declarations.version
    );
    if theirs != ours {
        found.push(Disagreement::new(
            "definition",
            format!("reads {ours}, given {theirs}"),
        ));
    }
}

fn types<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let shapes: Vec<&TypeShape> = declarations.types.iter().flat_map(|t| t.iter()).collect();
    // A generic declaration is a template no value reads as; each of its
    // instances is declared on its own.
    let declared: Vec<&TypeDecl> = def
        .types
        .iter()
        .filter(|(_, decl)| decl.site.is_some() && decl.parameters.is_empty())
        .map(|(_, decl)| decl)
        .collect();
    // Reference types are declared apart from the type catalog, so their
    // shapes are matched against the definition's references alone.
    let (references, shapes): (Vec<&TypeShape>, Vec<&TypeShape>) = shapes
        .into_iter()
        .partition(|shape| matches!(shape, TypeShape::Reference { .. }));
    for decl in &declared {
        let name = names.resolve(decl.name);
        if !shapes.iter().any(|shape| shape.name() == name) {
            found.push(Disagreement::new(
                format!("type {name}"),
                "not declared by the processor",
            ));
        }
    }
    for shape in shapes {
        let subject = format!("type {}", shape.name());
        let decl = declared
            .iter()
            .find(|decl| names.resolve(decl.name) == shape.name());
        match decl {
            None => found.push(Disagreement::new(subject, "not declared by the definition")),
            Some(decl) => {
                if let Some(reason) = type_shape(names, &decl.kind, shape) {
                    found.push(Disagreement::new(subject, reason));
                }
            }
        }
    }
    for decl in &def.references {
        let name = names.resolve(decl.name);
        if !references.iter().any(|shape| shape.name() == name) {
            found.push(Disagreement::new(
                format!("reference {name}"),
                "not declared by the processor",
            ));
        }
    }
    for shape in references {
        let subject = format!("reference {}", shape.name());
        let decl = def
            .references
            .iter()
            .find(|decl| names.resolve(decl.name) == shape.name());
        match decl {
            None => found.push(Disagreement::new(subject, "not declared by the definition")),
            Some(decl) => {
                if let Some(reason) = reference_shape(names, decl, shape) {
                    found.push(Disagreement::new(subject, reason));
                }
            }
        }
    }
}

/// How a reference type differs from its shape, if it does: the lookup
/// is the domain its references decode into.
fn reference_shape(names: &Names, decl: &ReferenceDecl, shape: &TypeShape) -> Option<String> {
    let TypeShape::Reference {
        target,
        lookup,
        qualifiers,
        ..
    } = shape
    else {
        return Some(format!(
            "declared as a reference, read as {}",
            shape_kind(shape)
        ));
    };
    let theirs = [
        format!("target {}", names.resolve(decl.target)),
        format!(
            "lookup {}",
            names.resolve(decl.lookup.unwrap_or(decl.target))
        ),
    ];
    let ours = [format!("target {target}"), format!("lookup {lookup}")];
    if let Some(reason) = differ("policy", &theirs, &ours) {
        return Some(reason);
    }
    let theirs: Vec<&str> = decl
        .qualifiers
        .iter()
        .map(|q| names.resolve(q.name))
        .collect();
    differ("qualifiers", &theirs, qualifiers)
}

/// How a type's compiled kind differs from its shape, if it does.
fn type_shape(names: &Names, kind: &TypeKind, shape: &TypeShape) -> Option<String> {
    match (kind, shape) {
        (TypeKind::String(_), TypeShape::String { .. }) => None,
        (TypeKind::Enum(enumeration), TypeShape::Enum { members, .. }) => {
            let theirs: Vec<&str> = enumeration
                .members
                .iter()
                .map(|m| m.name.as_str())
                .collect();
            differ("members", &theirs, members)
        }
        (TypeKind::Object(object), TypeShape::Object { fields, .. }) => {
            let theirs: Vec<String> = object
                .fields
                .iter()
                .map(|f| format!("{}: {}", names.resolve(f.name), expr(names, &f.ty)))
                .collect();
            let ours: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, (f.expression)()))
                .collect();
            differ("fields", &theirs, &ours)
        }
        (TypeKind::Union(union), TypeShape::Union { variants, .. }) => {
            let theirs: Vec<String> = union
                .variants
                .iter()
                .map(|v| format!("{}: {}", names.resolve(v.discriminator), expr(names, &v.ty)))
                .collect();
            let ours: Vec<String> = variants
                .iter()
                .map(|v| format!("{}: {}", v.discriminator, (v.expression)()))
                .collect();
            differ("variants", &theirs, &ours)
        }
        (TypeKind::Codec(codec), TypeShape::Codec { output, .. }) => {
            let theirs = [expr(names, &codec.output)];
            differ("output", &theirs, &[output()])
        }
        (TypeKind::Map(map), TypeShape::Map { key, value, .. }) => {
            let theirs = [expr(names, &map.key), expr(names, &map.value)];
            differ("key and value", &theirs, &[key(), value()])
        }
        (kind, _) => Some(format!(
            "declared as {}, read as {}",
            kind_name(kind),
            shape_kind(shape)
        )),
    }
}

fn differ<A: AsRef<str>, B: AsRef<str>>(what: &str, theirs: &[A], ours: &[B]) -> Option<String> {
    let theirs: Vec<&str> = theirs.iter().map(AsRef::as_ref).collect();
    let ours: Vec<&str> = ours.iter().map(AsRef::as_ref).collect();
    (theirs != ours).then(|| {
        format!(
            "{what} [{}] declared, [{}] read",
            theirs.join(", "),
            ours.join(", ")
        )
    })
}

fn kind_name(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Builtin(_) => "a built-in",
        TypeKind::String(_) => "a string type",
        TypeKind::Enum(_) => "an enum",
        TypeKind::Object(_) => "an object",
        TypeKind::Union(_) => "a union",
        TypeKind::Map(_) => "a map type",
        TypeKind::Codec(_) => "a codec",
    }
}

fn shape_kind(shape: &TypeShape) -> &'static str {
    match shape {
        TypeShape::String { .. } => "a string type",
        TypeShape::Enum { .. } => "an enum",
        TypeShape::Object { .. } => "an object",
        TypeShape::Union { .. } => "a union",
        TypeShape::Reference { .. } => "a reference",
        TypeShape::Codec { .. } => "a codec",
        TypeShape::Map { .. } => "a map type",
    }
}

fn nodes<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let shapes: Vec<&NodeShape> = declarations.nodes.iter().flat_map(|n| n.iter()).collect();
    let decls: Vec<&NodeDecl> = def.bindings.iter().map(|(_, decl)| decl).collect();
    let placed = |decl: &NodeDecl, shape: &NodeShape| {
        names.resolve(decl.name) == shape.name
            && parents(names, decl) == shape.parents
            && decl.placement.at_root() == shape.at_root()
    };
    for decl in &decls {
        if !shapes.iter().any(|shape| placed(decl, shape)) {
            let subject = node_subject(
                names.resolve(decl.name),
                &parents(names, decl),
                decl.placement.at_root(),
            );
            found.push(Disagreement::new(subject, "not declared by the processor"));
        }
    }
    for shape in shapes {
        let subject = node_subject(shape.name, shape.parents, shape.at_root());
        let Some(decl) = decls.iter().find(|decl| placed(decl, shape)) else {
            found.push(Disagreement::new(subject, "not declared by the definition"));
            continue;
        };
        if decl.role != shape.role {
            let reason = format!(
                "role {} declared, {} read",
                role_name(decl.role),
                role_name(shape.role)
            );
            found.push(Disagreement::new(subject.clone(), reason));
        }
        let ty = decl.ty.map(|ty| names.resolve(ty));
        if ty != shape.ty {
            let reason = format!(
                "type {} declared, {} read",
                ty.unwrap_or("-"),
                shape.ty.unwrap_or("-")
            );
            found.push(Disagreement::new(subject, reason));
        }
    }
}

fn parents<'n>(names: &'n Names, decl: &NodeDecl) -> Vec<&'n str> {
    decl.placement
        .parents()
        .iter()
        .map(|p| names.resolve(*p))
        .collect()
}

fn node_subject(name: &str, parents: &[&str], at_root: bool) -> String {
    match (parents.is_empty(), at_root) {
        (true, _) => format!("node {name} at the root"),
        (false, false) => format!("node {name} under {}", parents.join(" ")),
        (false, true) => format!("node {name} at the root and under {}", parents.join(" ")),
    }
}

fn role_name(role: Option<Role>) -> &'static str {
    role.map_or("-", Role::name)
}

fn slots<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let theirs: Vec<String> = def
        .slots
        .iter()
        .map(|slot| {
            let namespace = slot.namespace.map_or("-", |ns| names.resolve(ns));
            format!(
                "{} {} namespace={namespace}",
                names.resolve(slot.name),
                expr(names, &slot.ty)
            )
        })
        .collect();
    let ours: Vec<String> = declarations
        .slots
        .iter()
        .flat_map(|s| s.iter())
        .map(|slot| {
            let namespace = slot.namespace.unwrap_or("-");
            format!(
                "{} {} namespace={namespace}",
                slot.name,
                (slot.expression)()
            )
        })
        .collect();
    if let Some(reason) = differ("slots", &theirs, &ours) {
        found.push(Disagreement::new("context", reason));
    }
}

/// Every record and declaration node has exactly one handler: its own,
/// or else the one for the role and type name its items carry; and every
/// handler has a node.
fn handlers<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let handlers: Vec<_> = declarations
        .handlers
        .iter()
        .flat_map(|h| h.iter())
        .map(|h| (h.role, h.ty, h.node))
        .collect();
    let mut wanted: Vec<(Role, &str, Vec<&str>)> = Vec::new();
    for (_, decl) in def.bindings.iter() {
        let Some(role @ (Role::Record | Role::Declaration)) = decl.role else {
            continue;
        };
        let node = names.resolve(decl.name);
        for ty in item_types(names, decl) {
            match wanted.iter_mut().find(|(r, t, _)| (*r, *t) == (role, ty)) {
                Some((_, _, nodes)) if !nodes.contains(&node) => nodes.push(node),
                Some(_) => {}
                None => wanted.push((role, ty, vec![node])),
            }
        }
    }
    let count = |role: Role, ty: &str, node: Option<&str>| {
        handlers
            .iter()
            .filter(|h| (h.0, h.1, h.2) == (role, ty, node))
            .count()
    };
    for (role, ty, nodes) in &wanted {
        let mut shared = false;
        for node in nodes {
            match count(*role, ty, Some(node)) {
                0 => shared = true,
                1 => {}
                more => found.push(Disagreement::new(
                    format!("{} items of node {node}", role_name(Some(*role))),
                    format!("{more} handlers, expected 1"),
                )),
            }
        }
        let typed = count(*role, ty, None);
        if shared && typed != 1 {
            let subject = format!("{} items of type {ty}", role_name(Some(*role)));
            found.push(Disagreement::new(
                subject,
                format!("{typed} handlers, expected 1"),
            ));
        } else if !shared && typed > 0 {
            let subject = format!("handler for {} items of type {ty}", role_name(Some(*role)));
            found.push(Disagreement::new(
                subject,
                "every node reads with its own handler",
            ));
        }
    }
    for (role, ty, node) in &handlers {
        let nodes = wanted
            .iter()
            .find(|(r, t, _)| (r, t) == (role, ty))
            .map(|(_, _, nodes)| nodes);
        match (node, nodes) {
            (None, Some(_)) => {}
            (Some(node), Some(nodes)) if nodes.contains(node) => {}
            (None, None) => {
                let subject = format!("handler for {} items of type {ty}", role_name(Some(*role)));
                found.push(Disagreement::new(subject, "no node yields such items"));
            }
            (Some(node), _) => {
                let subject = format!(
                    "handler for {} items of type {ty} from node {node}",
                    role_name(Some(*role))
                );
                found.push(Disagreement::new(subject, "no node yields such items"));
            }
        }
    }
}

/// The type names a node's items carry, as binding emits them: the type
/// a case or `otherwise` selects, else the node's own. A node no case
/// matches is rejected, so its own name needs no handler.
fn item_types<'n>(names: &'n Names, decl: &NodeDecl) -> Vec<&'n str> {
    let own = names.resolve(decl.ty.unwrap_or(decl.name));
    if decl.cases.is_empty() {
        return vec![own];
    }
    let selected = decl.cases.iter().map(|case| &case.ty);
    let mut types = Vec::new();
    for ty in selected.chain(&decl.otherwise) {
        let name = match ty {
            TypeExpr::Named(name) => names.resolve(*name),
            _ => own,
        };
        if !types.contains(&name) {
            types.push(name);
        }
    }
    types
}
