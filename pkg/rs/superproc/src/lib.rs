//! The reference typed reader for schema definitions: the hand-maintained
//! processor that generated processors mirror. It declares the dialect's
//! types, node bindings, and context; reading is the engine's.
pub mod bindings;
pub mod contexts;
pub mod processors;
pub mod types;

pub use processors::{DECLARATIONS, Item, reader};
