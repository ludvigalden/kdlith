use serde::{Deserialize, Serialize};

use crate::identity::Reference;
use crate::origin::{OriginId, Trace};
use crate::source::Location;

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
    Asset(Asset),
}

/// A location naming a file or directory that is not a KDL document,
/// shown as its source shows it: `$name/<path>` in a named source,
/// project-root relative in the local one, never the directory backing
/// a named source. Binding records the spelling; the reading resolves
/// it in the source of the document that wrote it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Asset {
    pub location: String,
    /// The source's name; empty for the local source.
    pub source: String,
    /// The content digest, when the asset type records one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip)]
    state: AssetState,
}

/// How far an asset value got.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum AssetState {
    /// Read back from a serialized form: no reading vouches for it.
    #[default]
    Detached,
    Written(Box<AssetSpelling>),
    /// Resolved by a reading over the source set this fingerprints.
    Resolved(u64),
}

/// An asset value as written, before the reading resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AssetSpelling {
    /// The declared asset type that governs its resolution.
    pub ty: String,
    pub text: String,
    pub site: Location,
}

impl Asset {
    /// An asset as written at `site`, typed by the asset type `ty`.
    pub fn written(ty: impl Into<String>, text: impl Into<String>, site: Location) -> Self {
        let text = text.into();
        Self {
            location: text.clone(),
            source: String::new(),
            digest: None,
            state: AssetState::Written(Box::new(AssetSpelling {
                ty: ty.into(),
                text,
                site,
            })),
        }
    }

    /// An asset resolved to `location` in `source` by a reading over the
    /// source set `set` fingerprints.
    pub fn resolved(
        location: impl Into<String>,
        source: impl Into<String>,
        digest: Option<String>,
        set: u64,
    ) -> Self {
        Self {
            location: location.into(),
            source: source.into(),
            digest,
            state: AssetState::Resolved(set),
        }
    }

    /// The spelling awaiting resolution; `None` once resolved.
    pub fn spelling(&self) -> Option<&AssetSpelling> {
        match &self.state {
            AssetState::Written(spelling) => Some(spelling),
            _ => None,
        }
    }

    /// The fingerprint of the source set that resolved the asset; `None`
    /// for an asset no reading resolved.
    pub fn source_set(&self) -> Option<u64> {
        match self.state {
            AssetState::Resolved(set) => Some(set),
            _ => None,
        }
    }
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
