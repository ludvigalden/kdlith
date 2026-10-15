//! What a typed processor declares about the definition it reads: its
//! logical types, node declarations, context slots, and the handlers
//! that turn bound items into its own values.

use crate::bindings::Role;
use crate::definition::DefinitionKind;
use crate::typed::value::{Fields, FromFields, LogicalType, Mismatch, read_fields};

/// The type expression a Rust type stands for, as the definition spells it.
pub type Expression = fn() -> String;

/// How a handler reads one bound item's fields into a processor's value.
pub type ReadFn<T> = for<'a> fn(Fields<'a>) -> Result<T, Mismatch>;

/// A logical type as a processor declares it.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum TypeShape {
    String {
        name: &'static str,
    },
    Enum {
        name: &'static str,
        members: &'static [&'static str],
    },
    Object {
        name: &'static str,
        fields: &'static [FieldShape],
    },
    Union {
        name: &'static str,
        variants: &'static [VariantShape],
    },
}

impl TypeShape {
    pub const fn string(name: &'static str) -> Self {
        Self::String { name }
    }

    pub const fn enumeration(name: &'static str, members: &'static [&'static str]) -> Self {
        Self::Enum { name, members }
    }

    pub const fn object(name: &'static str, fields: &'static [FieldShape]) -> Self {
        Self::Object { name, fields }
    }

    pub const fn union(name: &'static str, variants: &'static [VariantShape]) -> Self {
        Self::Union { name, variants }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::String { name }
            | Self::Enum { name, .. }
            | Self::Object { name, .. }
            | Self::Union { name, .. } => name,
        }
    }
}

/// One field of an object type: its name and the expression of its type.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FieldShape {
    pub name: &'static str,
    pub expression: Expression,
}

impl FieldShape {
    pub const fn new(name: &'static str, expression: Expression) -> Self {
        Self { name, expression }
    }
}

/// One variant of a union type: its discriminating node name and type.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct VariantShape {
    pub discriminator: &'static str,
    pub expression: Expression,
}

impl VariantShape {
    pub const fn new(discriminator: &'static str, expression: Expression) -> Self {
        Self {
            discriminator,
            expression,
        }
    }
}

/// A Rust type declared for one logical type of a definition.
pub trait Shaped: LogicalType {
    const SHAPE: TypeShape;
}

/// A node declaration: its name, placement, role, and logical type.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct NodeShape {
    pub name: &'static str,
    /// The parents it may appear under; empty for a root node.
    pub parents: &'static [&'static str],
    pub role: Option<Role>,
    pub ty: Option<&'static str>,
}

impl NodeShape {
    /// A root node with no role and no type.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            parents: &[],
            role: None,
            ty: None,
        }
    }

    pub const fn under(mut self, parents: &'static [&'static str]) -> Self {
        self.parents = parents;
        self
    }

    pub const fn record(self) -> Self {
        self.role(Role::Record)
    }

    pub const fn declaration(self) -> Self {
        self.role(Role::Declaration)
    }

    pub const fn contribution(self) -> Self {
        self.role(Role::Contribution)
    }

    pub const fn context_control(self) -> Self {
        self.role(Role::ContextControl)
    }

    pub const fn source_import(self) -> Self {
        self.role(Role::SourceImport)
    }

    pub const fn container(self) -> Self {
        self.role(Role::Container)
    }

    const fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub const fn ty(mut self, ty: &'static str) -> Self {
        self.ty = Some(ty);
        self
    }
}

/// A typed context slot.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SlotShape {
    pub name: &'static str,
    pub expression: Expression,
    pub namespace: Option<&'static str>,
}

impl SlotShape {
    pub const fn new(name: &'static str, expression: Expression) -> Self {
        Self {
            name,
            expression,
            namespace: None,
        }
    }

    pub const fn in_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }
}

/// Reads the bound items of one role and logical type: the role and
/// type name an item carries select its handler.
#[derive(Debug)]
#[non_exhaustive]
pub struct Handler<T> {
    pub role: Role,
    pub ty: &'static str,
    pub read: ReadFn<T>,
}

impl<T> Handler<T> {
    /// Read record items of `D`'s logical type as `D`.
    pub const fn record<D: Shaped + FromFields>() -> Self
    where
        T: From<D>,
    {
        Self {
            role: Role::Record,
            ty: D::SHAPE.name(),
            read: read_as::<D, T>,
        }
    }

    /// Read declaration items of `D`'s logical type as `D`.
    pub const fn declaration<D: Shaped + FromFields>() -> Self
    where
        T: From<D>,
    {
        Self {
            role: Role::Declaration,
            ty: D::SHAPE.name(),
            read: read_as::<D, T>,
        }
    }

    /// The same handler for items whose type name is `ty`: a node bound
    /// without `type=` yields items named after the node, which its
    /// node-local Rust type reads.
    #[must_use]
    pub const fn named(mut self, ty: &'static str) -> Self {
        self.ty = ty;
        self
    }
}

impl<T> Clone for Handler<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handler<T> {}

fn read_as<D: FromFields, T: From<D>>(fields: Fields<'_>) -> Result<T, Mismatch> {
    read_fields::<D>(fields).map(T::from)
}

/// Everything a typed processor declares about the one definition it
/// reads, grouped by contributing document. Conforming it to a compiled
/// definition proves the two agree before anything is read.
#[derive(Debug)]
#[non_exhaustive]
pub struct Declarations<T: 'static> {
    pub kind: DefinitionKind,
    pub name: &'static str,
    pub version: &'static str,
    pub types: &'static [&'static [TypeShape]],
    pub nodes: &'static [&'static [NodeShape]],
    pub slots: &'static [&'static [SlotShape]],
    pub handlers: &'static [&'static [Handler<T>]],
}

impl<T: 'static> Declarations<T> {
    /// The declarations of a processor for the definition `name` at
    /// `version`, with nothing declared yet.
    pub const fn new(kind: DefinitionKind, name: &'static str, version: &'static str) -> Self {
        Self {
            kind,
            name,
            version,
            types: &[],
            nodes: &[],
            slots: &[],
            handlers: &[],
        }
    }

    pub const fn types(mut self, types: &'static [&'static [TypeShape]]) -> Self {
        self.types = types;
        self
    }

    pub const fn nodes(mut self, nodes: &'static [&'static [NodeShape]]) -> Self {
        self.nodes = nodes;
        self
    }

    pub const fn slots(mut self, slots: &'static [&'static [SlotShape]]) -> Self {
        self.slots = slots;
        self
    }

    pub const fn handlers(mut self, handlers: &'static [&'static [Handler<T>]]) -> Self {
        self.handlers = handlers;
        self
    }
}
