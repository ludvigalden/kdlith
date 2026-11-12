//! Handlers for the items `context.kdl` declares: context slots.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{SlotDef, SourceContextDef};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::declaration::<SlotDef>(),
    Handler::record::<SourceContextDef>(),
];
