//! Conformance: a typed processor's declarations are checked against the
//! compiled definition before it reads anything, in both directions, so
//! the processor can neither miss nor invent a type, node, or slot.

use std::fmt;

use crate::bindings::{NodeDecl, Placement, Role};
use crate::definition::Definition;
use crate::names::Names;
use crate::summary::expr;
use crate::typed::shape::{Declarations, NodeShape, TypeShape};
use crate::types::{TypeDecl, TypeKind};

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
    let declared: Vec<&TypeDecl> = def
        .types
        .iter()
        .filter(|(_, decl)| decl.site.is_some())
        .map(|(_, decl)| decl)
        .collect();
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
    }
}

fn nodes<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let shapes: Vec<&NodeShape> = declarations.nodes.iter().flat_map(|n| n.iter()).collect();
    let decls: Vec<&NodeDecl> = def.bindings.iter().map(|(_, decl)| decl).collect();
    let placed = |decl: &NodeDecl, shape: &NodeShape| {
        names.resolve(decl.name) == shape.name && parents(names, decl) == shape.parents
    };
    for decl in &decls {
        if !shapes.iter().any(|shape| placed(decl, shape)) {
            let subject = node_subject(names.resolve(decl.name), &parents(names, decl));
            found.push(Disagreement::new(subject, "not declared by the processor"));
        }
    }
    for shape in shapes {
        let subject = node_subject(shape.name, shape.parents);
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
    match &decl.placement {
        Placement::Root => Vec::new(),
        Placement::Under(parents) => parents.iter().map(|p| names.resolve(*p)).collect(),
    }
}

fn node_subject(name: &str, parents: &[&str]) -> String {
    if parents.is_empty() {
        format!("node {name} at the root")
    } else {
        format!("node {name} under {}", parents.join(" "))
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

/// Every record and declaration node has exactly one handler for the
/// role and type name its items carry, and every handler has a node.
fn handlers<T>(found: &mut Vec<Disagreement>, def: &Definition, declarations: &Declarations<T>) {
    let names = &def.names;
    let handlers: Vec<_> = declarations
        .handlers
        .iter()
        .flat_map(|h| h.iter())
        .map(|h| (h.role, h.ty))
        .collect();
    let mut wanted: Vec<(Role, &str)> = Vec::new();
    for (_, decl) in def.bindings.iter() {
        let Some(role @ (Role::Record | Role::Declaration)) = decl.role else {
            continue;
        };
        let ty = names.resolve(decl.ty.unwrap_or(decl.name));
        if !wanted.contains(&(role, ty)) {
            wanted.push((role, ty));
        }
    }
    for (role, ty) in &wanted {
        let count = handlers.iter().filter(|h| **h == (*role, *ty)).count();
        if count != 1 {
            let subject = format!("{} items of type {ty}", role_name(Some(*role)));
            found.push(Disagreement::new(
                subject,
                format!("{count} handlers, expected 1"),
            ));
        }
    }
    for (role, ty) in &handlers {
        if !wanted.contains(&(*role, *ty)) {
            let subject = format!("handler for {} items of type {ty}", role_name(Some(*role)));
            found.push(Disagreement::new(subject, "no node yields such items"));
        }
    }
}
