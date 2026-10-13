//! The kdlith engine: loads definitions with their imports and compiles
//! them once into linked catalogs and a processing plan that processors
//! consume without reading definition KDL again.
mod bindings;
mod compile;
mod deferred;
mod definition;
mod diag;
mod extensions;
mod load;
mod names;
mod plan;
mod structure;
mod summary;
mod symbols;
mod syntax;
mod type_expr;
mod types;

pub use bindings::{
    Arity, BindingCatalog, ChildPlan, ChildRule, Collision, ContextPolicy, DuplicateKeys, Effect,
    EffectGroup, EffectTarget, FieldBinding, NodeDecl, NodeDeclId, ParentKey, Placement, Role,
    Scope, ScopeId, SetFrom, Source,
};
pub use compile::{
    Compiled, DIALECT_VERSION, compile_dialect, compile_dialect_scoped, compile_pair,
    compile_schema, compile_schema_scoped,
};
pub use definition::{
    CheckScope, Deferral, Definition, DefinitionKind, DocumentRule, ImportDecl, SlotDecl,
};
pub use diag::Site;
pub use extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
pub use load::{MAX_DEFINITION_BYTES, load_dialect, load_schema};
pub use names::{Names, Sym};
pub use plan::{ContextPlan, ContextRead, DeferredBinding, EffectStep, NodePlan, ProcessingPlan};
pub use structure::{Occurs, OrderRule, ScopeRules, ScopeState, Violation};
pub use symbols::{ContributionDescriptor, DeclarationDescriptor, ReferencePolicy, SymbolCatalog};
pub use syntax::ParsedDocument;
pub use types::{
    Builtin, Constraint, EnumMember, EnumType, FieldDef, FormatRule, MapType, ObjectType,
    StringFormat, StringType, TypeCatalog, TypeDecl, TypeExpr, TypeId, TypeKind, UnionType,
    ValidatorUse, Variant,
};
