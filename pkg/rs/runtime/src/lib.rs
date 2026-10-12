//! Binding frames, context operations, session machinery, and processor
//! interfaces shared by every kdlith processor, hand-written or generated.
mod context;
mod frame;
mod processor;
mod session;
mod source;

pub use context::{
    Context, ContextEffect, ContextError, ContextLayout, ContextTarget, FollowingEffects,
    HeaderEffects, NamespaceId, Scope, SlotId, SlotValue,
};
pub use processor::{Completion, Cx, Disposition, Processor};
pub use session::{Session, SessionError, SessionLimits};
pub use source::{EventSource, FnSource, Pulled, SourceEvent, Step, from_fn};
