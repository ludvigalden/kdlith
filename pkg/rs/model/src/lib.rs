//! Shared vocabulary for every kdlith crate: source coordinates, node and
//! symbol identities, origin records, bound values, and the processing
//! items a session yields. Data definitions only; no processing logic.
mod identity;
mod item;
mod origin;
mod resolution;
mod source;
mod value;

pub use identity::{Identity, NodeId, QualifiedName, Reference};
pub use item::{
    BoundContribution, BoundDeclaration, BoundItem, BoundRecord, ExpansionRequest, ImportRequest,
    Item, PendingBinding, Repeated, ResolvedContribution, ScopeCompletion, SourceDocument,
    ValidationState,
};
pub use origin::{Derivation, Origin, OriginId, Trace};
pub use resolution::{DuplicateTreatment, FieldMerge, MissingRequirement, ResolutionSelection};
pub use source::{DocumentRole, Location, Position, SourceDescriptor, SourceId, Span};
pub use value::{Asset, AssetSpelling, Entry, Field, Value};
