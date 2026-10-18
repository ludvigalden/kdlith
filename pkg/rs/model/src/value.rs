use serde::{Deserialize, Serialize};

use crate::identity::Reference;
use crate::origin::{OriginId, Trace};

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
    /// The reference the key spells, when the map's key type is a
    /// reference type; `key` is then its written form. Binding never
    /// resolves it; linking does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    pub value: Value,
}

impl Entry {
    pub fn new(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            reference: None,
            value,
        }
    }

    /// An entry keyed by a reference, under the key as written.
    pub fn referenced(reference: Reference, value: Value) -> Self {
        Self {
            key: reference.written.clone(),
            reference: Some(reference),
            value,
        }
    }

    /// The key as a bound value: its reference when it holds one.
    pub fn key_value(&self) -> Value {
        match &self.reference {
            Some(reference) => Value::Reference(reference.clone()),
            None => Value::String(self.key.clone()),
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
    /// How the value came to be; empty when untraced. Not part of the
    /// field's serialized form.
    #[serde(skip)]
    pub trace: Trace,
    /// The value is a redaction marker the output policy put in place
    /// of a sensitive value. Not part of the field's serialized form.
    #[serde(skip)]
    pub redacted: bool,
}

impl Field {
    pub fn new(name: impl Into<String>, value: Value, origin: OriginId) -> Self {
        Self {
            name: name.into(),
            value,
            origin,
            trace: Trace::default(),
            redacted: false,
        }
    }

    /// The field with the trace of how its value came to be, and
    /// whether its value is a redaction marker.
    #[must_use]
    pub fn with_provenance(mut self, trace: Trace, redacted: bool) -> Self {
        self.trace = trace;
        self.redacted = redacted;
        self
    }
}
