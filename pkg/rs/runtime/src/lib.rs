//! Binding frames, context operations, session machinery, and processor
//! interfaces shared by every kdlith processor, hand-written or generated.
mod cache;
mod context;
mod export;
mod frame;
mod handlers;
mod imports;
mod processor;
mod provenance;
mod resolution;
mod session;
mod source;

pub use cache::{
    CACHE_CANONICALIZATION, CACHE_FORMAT, CacheEntry, CacheScan, CleanOutcome, Derivation,
    EncodedEntry, EntryClass, EntryFault, MAX_ENTRY_BYTES, OwnedEntry, SNAPSHOT_CANONICALIZATION,
    SNAPSHOT_FORMAT, SnapshotEntry, SnapshotLookup, SnapshotMeta, SnapshotRecord, WriteOutcome,
    cache_key, clean_state, encode_entry, encode_snapshot, read_snapshot, scan_state,
    slot_layout_digest, snapshot_key, write_state,
};
pub use context::{
    Context, ContextEffect, ContextError, ContextLayout, ContextTarget, FollowingEffects,
    HeaderEffects, NamespaceId, Scope, SlotId, SlotValue, stable_hash, stable_hash_step,
};
pub use export::{Delivery, ExportWriter, write_ndjson};
pub use handlers::{HandlerCall, HandlerEffect, HandlerFn, Handlers};
pub use imports::{
    Admission, ImportFailure, Invocation, InvocationId, InvocationIdentity, Invocations,
    MAX_IMPORT_DEPTH, MAX_INVOCATIONS, Repeated, ResolvedStream,
};
pub use processor::{Completion, Cx, Disposition, Processor};
pub use provenance::{FieldProvenance, Provenance, SourceRevision, SourceRevisions, export_line};
pub use resolution::{
    ContributionPolicy, ContributionRecord, ContributionResolver, LAYERED, Layered, MergeShape,
    ObjectShape,
};
pub use session::{Session, SessionError, SessionLimits};
pub use source::{EventSource, FnSource, Pulled, SourceEvent, Step, from_fn};
