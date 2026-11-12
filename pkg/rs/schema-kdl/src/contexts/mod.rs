//! Context declarations for schema definitions: the meta-schema's own typed
//! context slots. The meta-schema declares none, so its documents bind with
//! an empty context.

use kdlith_engine::SlotShape;

/// Every context slot, by contributing document.
pub(crate) const SLOTS: &[&[SlotShape]] = &[];
