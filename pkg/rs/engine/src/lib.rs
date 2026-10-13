//! The kdlith engine: loads and compiles definitions once into linked
//! catalogs and a processing plan, binds documents through that plan,
//! and reads them into a typed processor's own types.
mod bind;
mod bindings;
mod compile;
mod decode;
mod deferred;
mod definition;
mod diag;
mod documents;
mod extensions;
mod names;
mod overlap;
mod plan;
mod structure;
mod summary;
mod symbols;
mod syntax;
mod type_expr;
mod typed;
mod types;

pub use bind::{BindFrame, Binder, BindingSession, EntryValue, NodeStart, ParseEvents};
pub use bindings::{
    Arity, BindingCatalog, ChildPlan, ChildRule, Collision, ContextPolicy, DuplicateKeys, Effect,
    EffectGroup, EffectTarget, FieldBinding, NodeDecl, NodeDeclId, ParentKey, Placement, Role,
    Scope, ScopeId, SetFrom, Source,
};
pub use compile::input::{
    ChildRuleInput, ConstraintInput, Construct, DeferralInput, DefinitionInput, DocumentInput,
    DocumentRuleInput, EffectGroupInput, EffectInput, FieldBindingInput, FieldInput, FormatInput,
    ImportInput, Located, MemberInput, NodeInput, SlotInput, SourceInput, TypeInput,
    ValidatorInput, VariantInput,
};
pub use compile::{DIALECT_VERSION, compile_dialect, compile_dialect_input, compile_schema_input};
pub use definition::{
    CheckScope, Deferral, Definition, DefinitionKind, DocumentRule, ImportDecl, SlotDecl,
};
pub use diag::Site;
pub use documents::MAX_DOCUMENT_BYTES;
pub use extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
pub use names::{Names, Sym};
pub use plan::{ContextPlan, ContextRead, DeferredBinding, EffectStep, NodePlan, ProcessingPlan};
pub use structure::{Occurs, OrderRule, ScopeRules, ScopeState, Violation};
pub use symbols::{ContributionDescriptor, DeclarationDescriptor, ReferencePolicy, SymbolCatalog};
pub use syntax::ParsedDocument;
pub use typed::{
    Declarations, Expression, FieldShape, Fields, FromFields, FromValue, Handler, LogicalType,
    Mismatch, NodeShape, OrderedMap, ReadError, ReadFn, Shaped, SlotShape, TypeShape, TypedItem,
    TypedReader, ValueExpr, VariantShape, member_value, object_value, read_fields, text_value,
    variant_value,
};
pub use types::{
    Builtin, Constraint, EnumMember, EnumType, FieldDef, FormatRule, MapType, ObjectType,
    StringFormat, StringType, TypeCatalog, TypeDecl, TypeExpr, TypeId, TypeKind, UnionType,
    ValidatorUse, Variant,
};

/// Paths the declaration macros expand to; not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use kdlith_model::Value;
    pub use serde;
}
