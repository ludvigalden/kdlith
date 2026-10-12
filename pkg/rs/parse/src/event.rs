//! Parse events and the values they carry, borrowed from the source.

use std::borrow::Cow;

use kdlith_diagnostic::ByteSpan;

/// An item with the byte span of the source text it was read from.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Spanned<T> {
    pub item: T,
    pub span: ByteSpan,
}

impl<T> Spanned<T> {
    pub fn new(item: T, span: ByteSpan) -> Self {
        Self { item, span }
    }
}

/// One step of a document's structure. A `Node` is closed by exactly one
/// `End`; a `Children` between them opens the node's child nodes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event<'src> {
    /// The start of a node: its optional type annotation and its name.
    Node {
        ty: Option<Cow<'src, str>>,
        name: Cow<'src, str>,
    },
    /// An argument or property of the open node, in source order.
    Entry(Entry<'src>),
    /// The start of the open node's children block.
    Children,
    /// The end of the open node, after its children if it has any.
    End,
}

/// An argument (no name) or a property (named) of a node.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Entry<'src> {
    pub name: Option<Cow<'src, str>>,
    pub ty: Option<Cow<'src, str>>,
    pub value: Value<'src>,
}

/// A KDL value. Strings borrow the source unless escapes or multi-line
/// dedenting forced a decoded copy.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value<'src> {
    String(Cow<'src, str>),
    Number(Number<'src>),
    Bool(bool),
    Null,
}

/// A number kept as its exact source spelling, never as a binary float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Number<'src> {
    text: &'src str,
    kind: NumberKind,
}

impl<'src> Number<'src> {
    pub(crate) fn new(text: &'src str, kind: NumberKind) -> Self {
        Self { text, kind }
    }

    /// The spelling as written, separators and radix prefix included.
    pub fn text(&self) -> &'src str {
        self.text
    }

    pub fn kind(&self) -> NumberKind {
        self.kind
    }
}

/// The lexical class of a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NumberKind {
    Integer(Radix),
    /// A decimal with a fraction or an exponent.
    Float,
    Infinity,
    NegativeInfinity,
    NotANumber,
}

/// The radix of an integer spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Radix {
    Binary,
    Octal,
    Decimal,
    Hexadecimal,
}

impl Radix {
    pub fn value(self) -> u32 {
        match self {
            Radix::Binary => 2,
            Radix::Octal => 8,
            Radix::Decimal => 10,
            Radix::Hexadecimal => 16,
        }
    }

    pub(crate) fn is_digit(self, byte: u8) -> bool {
        char::from(byte).is_digit(self.value())
    }
}
