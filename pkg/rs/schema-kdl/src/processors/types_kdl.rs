//! Handlers for the items `types.kdl` declares: type declarations.

use kdlith_engine::Handler;

use super::Item;
use crate::types::{AssetType, CodecType, EnumType, MapType, ObjectType, StringType, UnionType};

pub(crate) const HANDLERS: &[Handler<Item>] = &[
    Handler::declaration::<StringType>(),
    Handler::declaration::<EnumType>(),
    Handler::declaration::<ObjectType>(),
    Handler::declaration::<UnionType>(),
    Handler::declaration::<MapType>(),
    Handler::declaration::<CodecType>(),
    Handler::declaration::<AssetType>(),
];
