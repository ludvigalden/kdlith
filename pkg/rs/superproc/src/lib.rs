//! The reference typed reader for schema definitions: the hand-maintained
//! processor that generated processors mirror. It declares the dialect's
//! types, node bindings, and context; reading is the engine's.
pub mod bindings;
pub mod compile;
pub mod contexts;
pub mod processors;
pub mod types;

pub use compile::{load_dialect, load_schema, load_schema_anchored, seed};
pub use kdlith_engine::{ResolutionCall, SharedResolver, Unresolvable, ValueResolver};
pub use processors::{DECLARATIONS, Item, reader, reader_with_resolver};
