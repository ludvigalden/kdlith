use serde::{Deserialize, Serialize};

use crate::source::{Location, SourceId};

/// Handle to a shared origin record. Values point at origins by handle so
/// derivation chains are stored once, not copied into every value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OriginId(pub u32);

impl OriginId {
    /// The origin of a value derived from source metadata (a
    /// source-context provider such as the file's path stem) rather than
    /// from any node of the stream. Node indices are bounded far below
    /// `u32::MAX`, so the sentinel never collides with a real node.
    pub const SOURCE_METADATA: Self = Self(u32::MAX);
}

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
/// an operation from earlier origins, supplied by a named provider, or
/// resolved at a location through the registered value-resolution hook.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// A computed default the registered value resolver supplied at
    /// `location`, the node it completed.
    ///
    /// `providers` names the declared secret providers the call carried,
    /// in dependency order; empty when no provider backs the value.
    Resolved {
        location: Location,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        providers: Vec<String>,
    },
}

impl Origin {
    /// Where the origin was recorded; a provided value has no location.
    pub fn location(&self) -> Option<&Location> {
        match self {
            Origin::Literal { location }
            | Origin::Derived { location, .. }
            | Origin::Resolved { location, .. } => Some(location),
            Origin::Provided { .. } => None,
        }
    }
}

/// How one value came to be, as a small local origin table: the first
/// record is the value's own origin, and every derived record's inputs
/// index this same table. Derivations reference shared records by index
/// rather than copying whole chains into every value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace(Vec<Origin>);

impl Trace {
    /// A value written literally at `location`.
    pub fn literal(location: Location) -> Self {
        Self(vec![Origin::Literal { location }])
    }

    /// A value a named provider supplied.
    pub fn provided(provider: impl Into<String>) -> Self {
        Self(vec![Origin::Provided {
            provider: provider.into(),
        }])
    }

    /// A value the registered value resolver supplied at `location`,
    /// drawing on the declared secret providers `providers` names.
    pub fn resolved(location: Location, providers: Vec<String>) -> Self {
        Self(vec![Origin::Resolved {
            location,
            providers,
        }])
    }

    /// A value `operation` produced at `location` from `inputs`. Each
    /// input's records follow the new root, re-based so its own derived
    /// inputs still point at its own records; an empty input is skipped.
    pub fn derived(operation: Derivation, location: Location, inputs: Vec<Trace>) -> Self {
        let mut records = vec![Origin::Derived {
            operation,
            location,
            inputs: Vec::new(),
        }];
        let mut roots = Vec::new();
        for input in inputs {
            if input.0.is_empty() {
                continue;
            }
            let base = u32::try_from(records.len()).unwrap_or(u32::MAX);
            roots.push(OriginId(base));
            records.extend(input.0.into_iter().map(|origin| rebase(origin, base)));
        }
        if let Some(Origin::Derived { inputs, .. }) = records.first_mut() {
            *inputs = roots;
        }
        Self(records)
    }

    /// Whether nothing is known about the value's origin.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The records, the value's own origin first.
    pub fn origins(&self) -> &[Origin] {
        &self.0
    }

    /// Every location of the trace moved from one source identity to
    /// another: a replayed invocation's items attribute to the
    /// invocation that replayed them.
    pub fn relocate(&mut self, from: SourceId, to: SourceId) {
        for origin in &mut self.0 {
            match origin {
                Origin::Literal { location }
                | Origin::Derived { location, .. }
                | Origin::Resolved { location, .. } => {
                    location.relocate(from, to);
                }
                Origin::Provided { .. } => {}
            }
        }
    }
}

/// An origin whose derived inputs move `base` records further down a
/// combined table.
fn rebase(origin: Origin, base: u32) -> Origin {
    match origin {
        Origin::Derived {
            operation,
            location,
            inputs,
        } => Origin::Derived {
            operation,
            location,
            inputs: inputs
                .into_iter()
                .map(|OriginId(at)| OriginId(at.saturating_add(base)))
                .collect(),
        },
        other => other,
    }
}
