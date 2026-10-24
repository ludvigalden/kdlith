//! The kdlith engine: loads and compiles definitions once into linked
//! catalogs and a processing plan, binds documents through that plan,
//! and reads them into a typed processor's own types.
mod assets;
mod bind;
mod bindings;
mod cache;
mod compile;
mod data;
mod decode;
mod definition;
mod diag;
mod digest;
mod documents;
mod export;
mod extensions;
mod link;
mod list;
mod locations;
mod manifest;
mod names;
mod overlap;
mod plan;
mod resolution;
mod snapshot_store;
mod snapshots;
mod source_context;
mod sources;
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
    Arity, BindingCatalog, BindingCodec, CaseRule, ChildPlan, ChildRule, Collision, Condition,
    ContextPolicy, DependentBinding, DescendantRule, DuplicateKeys, Effect, EffectGroup,
    EffectTarget, FieldBinding, IdentityDecl, NodeAnnotation, NodeDecl, NodeDeclId, ParentKey,
    Placement, Role, Scope, ScopeId, SequenceDecl, SequenceStep, SetFrom, Source as FieldSource,
    WhenBranch,
};
pub use cache::{DerivedState, derive_state};
pub use compile::input::{
    AnnotationInput, CaptureInput, CaseInput, ChildRuleInput, ConditionInput, ConstraintInput,
    Construct, DefaultInput, DefinitionInput, DependentInput, DescendantRuleInput, DocumentInput,
    DocumentRuleInput, EffectGroupInput, EffectInput, FieldBindingInput, FieldInput, FormatInput,
    GroupInput, IdentityInput, ImportInput, Located, LocationsInput, MatchRuleInput, MemberInput,
    NodeAnnotationInput, NodeInput, ProfileInput, ProviderInput, QualifierInput, ReferenceInput,
    ResolutionInput, SequenceInput, SlotInput, SourceContextInput, SourceFieldInput, SourceInput,
    StepInput, TypeInput, ValidatorInput, VariantInput,
};
pub use compile::{DIALECT_VERSION, compile_dialect, compile_dialect_input, compile_schema_input};
pub use data::{
    BoundResult, DataReader, EntryContext, ReadOptions, RecordContext, Restored, Source,
};
pub use definition::{
    AccumulateOp, AnnotationDecl, CheckScope, Definition, DefinitionKind, DocumentRule,
    DuplicateTreatment, ImportDecl, LocationsDecl, MatchKind, MatchRule, MergeDuplicates,
    MissingRequirement, Patterns, ProfileDecl, ProfileSelection, ProviderDecl, Qualification,
    QualifierDecl, ReferenceDecl, ResolutionPolicyDecl, ResolutionSelection, SlotDecl,
    SlotDuplicates, SourceContextDecl, SourceContextMode, SourceFieldDecl, match_path,
};
pub use diag::Site;
pub use digest::tree_digest;
pub use documents::MAX_DOCUMENT_BYTES;
pub use export::{Emit, ExportOutcome, export, export_read, export_under};
pub use extensions::{
    Extensions, PATH_STEM_SOURCE, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION,
};
pub use kdlith_model::ResolvedContribution;
/// Deliberate root surface: tools name `Value` through the engine and
/// the generated crates that re-export it, never through `__private`.
pub use kdlith_model::{Asset, Value};
pub use kdlith_runtime::{
    CacheEntry, CacheScan, CleanOutcome, Derivation, EncodedEntry, EntryClass, EntryFault,
    OwnedEntry, SnapshotEntry, SnapshotMeta, SnapshotRecord, WriteOutcome, cache_key, clean_state,
    scan_state, snapshot_key, write_state,
};
pub use kdlith_runtime::{
    ContributionPolicy, ContributionRecord, ContributionResolver, LAYERED, Layered,
};
pub use kdlith_runtime::{
    Delivery, FieldProvenance, HandlerCall, HandlerEffect, HandlerFn, Handlers, Provenance,
    SlotValue, SourceRevision, SourceRevisions, export_line,
};
pub use link::{
    BoundReference, DuplicateDeclaration, IdentityIndex, IndexedDeclaration, Link, LinkInputs,
    LinkOutcome, LinkReport, Lookup, ReferenceLinks, UnmetQualifier, bind_references,
    derive_candidates, index_identities, link, link_reading,
};
pub use list::{
    IndexEntry, IndexKind, ItemIndex, NodeEntry, ProfileEntry, TypeEntry, index_items, list_nodes,
    list_profiles, list_types, record_identity,
};
pub use locations::{SourceTree, is_literal, pattern_names};
pub use manifest::SourceOrigin;
pub use names::{Names, Sym};
pub use plan::{ContextPlan, ContextRead, DeferredBinding, EffectStep, NodePlan, ProcessingPlan};
pub use resolution::{ResolutionError, SharedContributionResolver, resolve};
pub use snapshot_store::{SnapshotLoad, encode_snapshot_entry, load_snapshot};
pub use snapshots::{
    ContextSnapshot, SNAPSHOT_MISMATCH, SnapshotError, SnapshotIndex, SnapshotIndexEntry,
    ValidationStatus as SnapshotValidation, definition_layout_digest, snapshot_fingerprint,
};
pub use source_context::{AppliedSourceValue, apply as apply_source_context};
pub use sources::{SourcesReport, list_sources, plan_sources, verify_sources};
pub use structure::{Occurs, OrderRule, ScopeRules, ScopeState, Violation};
pub use symbols::{ContributionDescriptor, DeclarationDescriptor, ReferencePolicy, SymbolCatalog};
pub use syntax::ParsedDocument;
pub use typed::{
    Decimal, Declarations, Expression, FieldShape, Fields, FromFields, FromValue, Handler,
    ItemVariant, LogicalType, Mismatch, NodeShape, Nullable, OpenError, OrderedMap, Origin,
    ReadError, ReadFn, Reading, Ref, ResolveError, Shaped, SlotShape, TypeShape, TypedItem,
    TypedReader, ValueExpr, VariantShape, member_value, object_value, read_fields, reference_value,
    text_value, variant_value,
};
pub use types::{
    AssetBase, AssetDecl, AssetKind, Builtin, CaptureDef, CodecDecl, Constraint, DefaultSpec,
    EnumMember, EnumType, ExpandedGroup, FieldDef, FormatRule, MapType, ObjectType, OutputPolicy,
    Sensitivity, StringFormat, StringType, TypeCatalog, TypeDecl, TypeExpr, TypeId, TypeKind,
    UnionType, ValidatorUse, Variant,
};

pub use value_resolver::{ResolutionCall, SharedResolver, Unresolvable, ValueResolver};

/// Paths the declaration macros expand to; not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use kdlith_model::Value;
    pub use serde;
}
