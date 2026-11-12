//! The dialect's logical types: one module per contributing document,
//! each type declared once and read from bound values.

mod bindings_kdl;
mod context_kdl;
mod documents_kdl;
mod types_kdl;

use kdlith_engine::TypeShape;

pub use bindings_kdl::*;
pub use context_kdl::*;
pub use documents_kdl::*;
pub use types_kdl::*;

/// Every type's shape, by contributing document.
pub(crate) const TYPES: &[&[TypeShape]] = &[
    types_kdl::TYPES,
    bindings_kdl::TYPES,
    context_kdl::TYPES,
    documents_kdl::TYPES,
];
