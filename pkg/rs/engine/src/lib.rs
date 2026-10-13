//! The kdlith engine: compiles a dialect and a schema definition once
//! into catalogs (types, node bindings, per-scope structural rules) that
//! every processor consumes without reading definition KDL again.
mod bindings;
mod compile;
mod deferred;
mod definition;
mod diag;
mod extensions;
mod names;
mod structure;
mod syntax;
mod type_expr;
mod types;

pub use bindings::{
    Arity, BindingCatalog, ChildPlan, ChildRule, Collision, ContextPolicy, DuplicateKeys, Effect,
    EffectGroup, EffectTarget, FieldBinding, NodeDecl, NodeDeclId, ParentKey, Placement, Role,
    Scope, ScopeId, SetFrom, Source,
};
pub use compile::{Compiled, DIALECT_VERSION, compile_dialect, compile_pair, compile_schema};
pub use definition::{
    Deferral, Definition, DefinitionKind, DocumentRule, ImportDecl, Repeated, SlotDecl,
};
pub use diag::Site;
pub use extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
pub use names::{Names, Sym};
pub use structure::{Occurs, OrderRule, ScopeRules, ScopeState, Violation};
pub use syntax::ParsedDocument;
pub use types::{
    Builtin, Constraint, EnumMember, EnumType, FieldDef, FormatRule, MapType, ObjectType,
    StringFormat, StringType, TypeCatalog, TypeDecl, TypeExpr, TypeId, TypeKind, UnionType,
    ValidatorUse, Variant,
};
