//! Typed readers: the per-role schema-definition processing logic. The
//! dialect's items, the handlers that read them, and the declarations
//! the engine conforms to the compiled dialect before reading.

mod bindings_kdl;
mod context_kdl;
mod documents_kdl;
mod types_kdl;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_engine::{
    Declarations, Definition, DefinitionKind, Handler, SharedContributionResolver, SharedResolver,
    TypedReader, item_type,
};

use crate::types::{
    AnnotationDef, CodecType, DiscoveryDef, DocumentRule, EnumType, MapType, NodeDef, ObjectType,
    ProfileDef, ReferenceDef, ResolutionPolicyDef, SecretProvider, SlotDef, SourceContextDef,
    StringType, UnionType,
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
        AnnotationDef,
        SlotDef,
        SourceContextDef,
        DocumentRule,
        ProfileDef,
        DiscoveryDef,
        ReferenceDef,
        ResolutionPolicyDef,
        SecretProvider,
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

/// A reader of definitions written in the compiled dialect `dialect`
/// whose computed defaults resolve through the consuming program's
/// value resolver `resolver`, which receives the declared descriptors
/// as data.
pub fn reader_with_resolver(
    dialect: &Definition,
    resolver: SharedResolver,
) -> Result<TypedReader<'_, Item>, Vec<DiagnosticRecord>> {
    reader(dialect).map(|typed| typed.with_resolver(resolver))
}

/// A reader of definitions written in the compiled dialect `dialect`
/// whose contributions resolve through the processor's contribution
/// resolver `resolver` under the declared resolution policy.
pub fn reader_with_contribution_resolver(
    dialect: &Definition,
    resolver: SharedContributionResolver,
) -> Result<TypedReader<'_, Item>, Vec<DiagnosticRecord>> {
    reader(dialect).map(|typed| typed.with_contribution_resolver(resolver))
}
