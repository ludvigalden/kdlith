//! Context declarations for schema definitions: the dialect's own typed
//! context slots. The dialect declares none, so its documents bind with
//! an empty context.

use kdlith_engine::SlotShape;

/// Every context slot, by contributing document.
pub(crate) const SLOTS: &[&[SlotShape]] = &[];
