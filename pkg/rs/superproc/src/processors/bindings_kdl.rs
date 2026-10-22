//! Handlers for the items `bindings.kdl` declares: node declarations.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{AnnotationDef, NodeDef};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::declaration::<NodeDef>(),
    Handler::declaration::<AnnotationDef>(),
];
