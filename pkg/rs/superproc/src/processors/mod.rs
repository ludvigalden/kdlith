//! Typed readers: the per-role schema-definition processing logic. The
//! dialect's items, the handlers that read them, and the declarations
//! the engine conforms to the compiled dialect before reading.

mod bindings_kdl;
mod context_kdl;
mod documents_kdl;
mod types_kdl;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_engine::{Declarations, Definition, DefinitionKind, Handler, TypedReader, item_type};

use crate::types::{
    CodecType, Deferral, DocumentRule, EnumType, MapType, NodeDef, ObjectType, SlotDef, StringType,
    UnionType,
};
use crate::{bindings, contexts, types};

item_type! {
    /// A record or declaration of a definition written in the dialect.
    pub enum Item {
        StringType,
        EnumType,
        ObjectType,
        UnionType,
        MapType,
        CodecType,
        NodeDef,
        SlotDef,
        DocumentRule,
        Deferral,
    }
}

/// Every handler, by contributing document.
const HANDLERS: &[&[Handler<Item>]] = &[
    types_kdl::HANDLERS,
    bindings_kdl::HANDLERS,
    context_kdl::HANDLERS,
    documents_kdl::HANDLERS,
];

/// Everything this processor declares about the dialect it reads.
pub static DECLARATIONS: Declarations<Item> =
    Declarations::new(DefinitionKind::Dialect, "kdlith", "0")
        .types(types::TYPES)
        .nodes(bindings::NODES)
        .slots(contexts::SLOTS)
        .handlers(HANDLERS);

/// A reader of definitions written in the compiled dialect `dialect`.
pub fn reader(dialect: &Definition) -> Result<TypedReader<'_, Item>, Vec<DiagnosticRecord>> {
    TypedReader::new(dialect, &DECLARATIONS)
}
