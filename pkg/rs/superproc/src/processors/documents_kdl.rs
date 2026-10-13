//! Handlers for the items `documents.kdl` declares: document rule
//! records and deferral declarations.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{Deferral, DocumentRule};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::record::<DocumentRule>(),
    Handler::declaration::<Deferral>(),
];
