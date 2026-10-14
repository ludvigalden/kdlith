//! Handlers for the items `documents.kdl` declares: document rule
//! records and the declarations the documents surface adds.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{
    DiscoveryDef, DocumentRule, ProfileDef, ReferenceDef, ResolutionPolicyDef, SecretProvider,
};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::record::<DocumentRule>(),
    Handler::declaration::<ProfileDef>(),
    Handler::declaration::<DiscoveryDef>(),
    Handler::declaration::<ReferenceDef>(),
    Handler::declaration::<ResolutionPolicyDef>(),
    Handler::declaration::<SecretProvider>(),
];
