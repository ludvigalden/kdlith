use serde::{Deserialize, Serialize};

use crate::source::Location;

/// Handle to a shared origin record. Values point at origins by handle so
/// derivation chains are stored once, not copied into every value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OriginId(pub u32);

/// The operation that produced a derived value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Derivation {
    Reference,
    Override,
    Merge,
    Expansion,
    Default,
    Context,
    Import,
}

/// Where a value came from: written literally at a location, derived by
/// an operation from earlier origins, or supplied by a named provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Origin {
    Literal {
        location: Location,
    },
    Derived {
        operation: Derivation,
        location: Location,
        inputs: Vec<OriginId>,
    },
    Provided {
        provider: String,
    },
}
