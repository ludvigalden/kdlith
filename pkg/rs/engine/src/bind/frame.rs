//! The binding state of one open node, kept on the session's bounded
//! frame stack until the node closes.

use kdlith_diagnostic::ByteSpan;
use kdlith_model::Value;

use crate::bindings::{NodeDeclId, ScopeId};
use crate::names::Sym;
use crate::structure::ScopeState;

use super::tables::{Bind, Table, Tables};

/// How an open node binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Through its own declaration; `key` is the argument an entries
    /// envelope consumes as the map key.
    Declared { decl: NodeDeclId, key: Option<u64> },
    /// Through the nested sources of the owner's fields.
    Nested,
    /// As exactly one positional value of an owner field.
    Scalar,
}

/// A value supplied to a field, with the span it came from.
#[derive(Clone, Debug)]
pub(crate) struct Supplied {
    pub(crate) value: Value,
    pub(crate) span: ByteSpan,
}

impl Supplied {
    pub(crate) fn new(value: Value, span: ByteSpan) -> Self {
        Self { value, span }
    }
}

/// A supply on its way up to the declared node owning the field.
pub(crate) type Carried = (Bind, Value, ByteSpan);

/// One open node's binding state.
#[derive(Debug)]
pub struct BindFrame {
    pub(crate) name: Sym,
    /// The declaration whose fields this node's syntax supplies.
    pub(crate) owner: NodeDeclId,
    pub(crate) kind: Kind,
    /// The scope admitting this node's children; `None` admits none.
    pub(crate) scope: Option<ScopeId>,
    pub(crate) state: Option<ScopeState>,
    pub(crate) span: ByteSpan,
    /// Positional arguments seen so far.
    pub(crate) args: u64,
    pub(crate) props: Vec<Sym>,
    /// Declared nodes: the owner's fields, by field position.
    pub(crate) fields: Vec<Option<Supplied>>,
    /// Nested and scalar nodes: supplies carried up to the owner.
    pub(crate) carried: Vec<Carried>,
    /// Owner fields whose supplied value was rejected, so the field is
    /// not also reported missing.
    pub(crate) rejected: Vec<usize>,
    /// The entries-envelope key, once argument `key` arrives.
    pub(crate) key: Option<(String, ByteSpan)>,
    /// A scalar node's single value.
    pub(crate) scalar: Option<(Value, ByteSpan)>,
    /// The binder's error count when the node opened.
    pub(crate) mark: u64,
    /// Errors of descendants that are processing items of their own.
    pub(crate) excluded: u64,
}

/// What a new frame is made of.
pub(crate) struct Opened {
    pub(crate) name: Sym,
    pub(crate) owner: NodeDeclId,
    pub(crate) kind: Kind,
    pub(crate) scope: Option<ScopeId>,
    pub(crate) state: Option<ScopeState>,
    pub(crate) span: ByteSpan,
    pub(crate) width: usize,
    pub(crate) mark: u64,
}

impl BindFrame {
    pub(crate) fn new(opened: Opened) -> Self {
        let mut fields = Vec::new();
        fields.resize_with(opened.width, || None);
        Self {
            name: opened.name,
            owner: opened.owner,
            kind: opened.kind,
            scope: opened.scope,
            state: opened.state,
            span: opened.span,
            args: 0,
            props: Vec::new(),
            fields,
            carried: Vec::new(),
            rejected: Vec::new(),
            key: None,
            scalar: None,
            mark: opened.mark,
            excluded: 0,
        }
    }

    /// The table mapping this node's syntax to owner fields.
    pub(crate) fn table<'t>(&self, tables: &'t Tables) -> Option<&'t Table> {
        tables.scopes.get(self.scope?.index())?.as_ref()
    }

    pub(crate) fn declared(&self) -> bool {
        matches!(self.kind, Kind::Declared { .. })
    }
}
