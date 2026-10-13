//! Handlers for the items `documents.kdl` declares: document rules and
//! deferrals.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{Deferral, DocumentRule};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::declaration::<DocumentRule>(),
    Handler::declaration::<Deferral>(),
];
