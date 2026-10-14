//! Binding frames, context operations, session machinery, and processor
//! interfaces shared by every kdlith processor, hand-written or generated.
mod context;
mod export;
mod frame;
mod handlers;
mod imports;
mod processor;
mod session;
mod source;

pub use context::{
    Context, ContextEffect, ContextError, ContextLayout, ContextTarget, FollowingEffects,
    HeaderEffects, NamespaceId, Scope, SlotId, SlotValue, stable_hash, stable_hash_step,
};
pub use export::{Delivery, ExportWriter, write_ndjson};
pub use handlers::{HandlerCall, HandlerEffect, HandlerFn, Handlers};
pub use imports::{
    Admission, ImportFailure, Invocation, InvocationId, InvocationIdentity, Invocations,
    MAX_IMPORT_DEPTH, MAX_INVOCATIONS, Repeated,
};
pub use processor::{Completion, Cx, Disposition, Processor};
pub use session::{Session, SessionError, SessionLimits};
pub use source::{EventSource, FnSource, Pulled, SourceEvent, Step, from_fn};
