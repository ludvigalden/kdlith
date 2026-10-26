use serde::{Deserialize, Serialize};

use crate::source::{Location, SourceId};

/// Positional identity of one node: its source and its zero-based index
/// in document order. Distinct from any symbol the node declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NodeId {
    pub source: SourceId,
    pub index: u32,
}

impl NodeId {
    pub fn new(source: SourceId, index: u32) -> Self {
        Self { source, index }
    }
}

/// A name of one or more segments, outermost qualifier first. How it is
/// spelled in source is a codec concern, not part of the name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct QualifiedName(pub Vec<String>);

/// A declared symbol: a qualified name within a named identity domain.
/// Identities are literal and stable; they are never computed at lookup.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Identity {
    pub domain: String,
    pub name: QualifiedName,
}

impl Identity {
    pub fn new(domain: impl Into<String>, name: QualifiedName) -> Self {
        Self {
            domain: domain.into(),
            name,
        }
    }
}

/// A symbolic reference as written: the domain it looks up in, the
/// parsed name, exact qualifiers, and the original spelling and site.
/// Binding a reference never resolves it; linking is a separate step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Reference {
    pub domain: String,
    pub name: QualifiedName,
    pub qualifiers: Vec<(String, String)>,
    pub written: String,
    pub site: Location,
    /// The qualifier-context value in effect where the reference was
    /// bound, for a reference whose qualification may come from
    /// context: the value of the slot its reference type or field
    /// chose through `qualify-from`, else the `namespace` slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Box<str>>,
}

impl Reference {
    pub fn new(
        domain: impl Into<String>,
        name: QualifiedName,
        written: impl Into<String>,
        site: Location,
    ) -> Self {
        Self {
            domain: domain.into(),
            name,
            qualifiers: Vec::new(),
            written: written.into(),
            site,
            context: None,
        }
    }
}
