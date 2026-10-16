//! The kdlith engine: loads and compiles definitions once into linked
//! catalogs and a processing plan, binds documents through that plan,
//! and reads them into a typed processor's own types.
mod anchors;
mod bind;
mod bindings;
mod cache;
mod compile;
mod data;
mod decode;
mod definition;
mod diag;
mod discovery;
mod documents;
mod export;
mod extensions;
mod list;
mod names;
mod overlap;
mod plan;
mod resolution;
mod snapshots;
mod source_context;
mod structure;
mod summary;
mod symbols;
mod syntax;
mod type_expr;
mod typed;
mod types;
mod value_resolver;

pub use bind::{BindFrame, Binder, BindingSession, EntryValue, NodeStart, ParseEvents};
pub use bindings::{
    Arity, BindingCatalog, BindingCodec, CaseRule, ChildPlan, ChildRule, Collision, ContextPolicy,
    DependentBinding, DescendantRule, DuplicateKeys, Effect, EffectGroup, EffectTarget,
    FieldBinding, IdentityDecl, NodeDecl, NodeDeclId, ParentKey, Placement, Role, Scope, ScopeId,
    SequenceDecl, SequenceStep, SetFrom, Source,
};
pub use cache::{DerivedState, derive_state};
pub use compile::input::{
    AnchorInput, CaptureInput, CaseInput, ChildRuleInput, ConstraintInput, Construct, DefaultInput,
    DefinitionInput, DependentInput, DescendantRuleInput, DiscoveryInput, DocumentInput,
    DocumentRuleInput, EffectGroupInput, EffectInput, FieldBindingInput, FieldInput, FormatInput,
    IdentityInput, ImportInput, Located, MatchRuleInput, MemberInput, NodeInput, ProfileInput,
    ProviderInput, QualifierInput, ReferenceInput, ResolutionInput, SequenceInput, SlotInput,
    SourceContextInput, SourceFieldInput, SourceInput, StepInput, TypeInput, ValidatorInput,
    VariantInput,
};
pub use compile::{DIALECT_VERSION, compile_dialect, compile_dialect_input, compile_schema_input};
pub use data::{BoundResult, DataReader, ReadOptions};
pub use definition::{
    AccumulateOp, AnchorDecl, CheckScope, Definition, DefinitionKind, DiscoveryDecl, DocumentRule,
    DuplicateTreatment, ImportDecl, MatchKind, MatchRule, MergeDuplicates, MissingRequirement,
    Patterns, ProfileDecl, ProfileSelection, ProviderDecl, Qualification, QualifierDecl,
    ReferenceDecl, ResolutionPolicyDecl, ResolutionSelection, SlotDecl, SlotDuplicates,
    SourceContextDecl, SourceContextMode, SourceFieldDecl,
};
pub use diag::Site;
pub use discovery::{Expansion, ExpansionError, Segment, expand, is_expression, parse};
pub use documents::MAX_DOCUMENT_BYTES;
pub use export::{Emit, ExportOutcome, export, export_read, export_under};
pub use extensions::{
    Extensions, PATH_STEM_SOURCE, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION,
};
pub use kdlith_runtime::{
    CacheEntry, CacheScan, CleanOutcome, Derivation, EncodedEntry, EntryFault, WriteOutcome,
    cache_key, clean_state, scan_state, write_state,
};
pub use kdlith_runtime::{
    Delivery, FieldProvenance, HandlerCall, HandlerEffect, HandlerFn, Handlers, Provenance,
    SourceRevision, SourceRevisions, export_line,
};
pub use list::{
    IndexEntry, IndexKind, ItemIndex, NodeEntry, ProfileEntry, TypeEntry, index_items, list_nodes,
    list_profiles, list_types, record_identity,
};
pub use names::{Names, Sym};
pub use plan::{ContextPlan, ContextRead, DeferredBinding, EffectStep, NodePlan, ProcessingPlan};
pub use resolution::{
    ContributionRecord, ContributionResolver, ResolutionError, ResolvedContribution, resolve,
};
pub use snapshots::{
    ContextSnapshot, SNAPSHOT_MISMATCH, SnapshotError, SnapshotIndex, SnapshotIndexEntry,
    ValidationStatus as SnapshotValidation,
};
pub use source_context::{AppliedSourceValue, apply as apply_source_context};
pub use structure::{Occurs, OrderRule, ScopeRules, ScopeState, Violation};
pub use symbols::{ContributionDescriptor, DeclarationDescriptor, ReferencePolicy, SymbolCatalog};
pub use syntax::ParsedDocument;
pub use typed::{
    Declarations, Expression, FieldShape, Fields, FromFields, FromValue, Handler, ItemVariant,
    Links, LogicalType, Mismatch, NodeShape, OrderedMap, ReadError, ReadFn, Ref, Shaped, SlotShape,
    TypeShape, TypedItem, TypedReader, Unresolved, ValueExpr, VariantShape, member_value,
    object_value, read_fields, text_value, variant_value,
};
pub use types::{
    Builtin, CaptureDef, CodecDecl, Constraint, DefaultSpec, EnumMember, EnumType, FieldDef,
    FormatRule, MapType, ObjectType, OutputPolicy, Sensitivity, StringFormat, StringType,
    TypeCatalog, TypeDecl, TypeExpr, TypeId, TypeKind, UnionType, ValidatorUse, Variant,
};

pub use value_resolver::{ResolutionCall, SharedResolver, Unresolvable, ValueResolver};

/// Paths the declaration macros expand to; not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use kdlith_model::Value;
    pub use serde;
}
