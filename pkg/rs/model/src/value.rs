use serde::{Deserialize, Serialize};

use crate::identity::Reference;
use crate::origin::OriginId;

/// A bound value. Numbers keep their exact source spelling, never a
/// binary float; maps keep declaration order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    List(Vec<Value>),
    Map(Vec<Entry>),
    Reference(Reference),
}

/// One keyed member of an ordered map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Entry {
    pub key: String,
    pub value: Value,
}

impl Entry {
    pub fn new(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}

/// One named field of a bound item with the origin of its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Field {
    pub name: String,
    pub value: Value,
    pub origin: OriginId,
}

impl Field {
    pub fn new(name: impl Into<String>, value: Value, origin: OriginId) -> Self {
        Self {
            name: name.into(),
            value,
            origin,
        }
    }
}
