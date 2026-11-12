//! Node, property, argument, and child bindings for schema definitions:
//! the dialect's node declarations, one table per contributing document.

mod bindings_kdl;
mod context_kdl;
mod documents_kdl;
mod types_kdl;

use kdlith_engine::NodeShape;

/// Every node declaration, by contributing document.
pub(crate) const NODES: &[&[NodeShape]] = &[
    types_kdl::NODES,
    bindings_kdl::NODES,
    context_kdl::NODES,
    documents_kdl::NODES,
];
