//! The sources a reading used, as its provenance reports them: one
//! entry per source with the identity of the tree it read.

use serde::Serialize;

/// Where an origin source's tree comes from: a git repository, the
/// revision, and the directory in it. kdlith never fetches; the origin
/// says what to fetch into the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceOrigin {
    pub git: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

impl SourceOrigin {
    pub fn new(git: impl Into<String>, path: Option<String>, rev: Option<String>) -> Self {
        Self {
            git: git.into(),
            path,
            rev,
        }
    }
}

/// What kind of tree a source is declared as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SourceKind {
    /// The reading's own tree: the project root, or an entry's directory.
    Local,
    Dir,
    Origin,
    Embedded,
}

/// The override that replaced a source's declared tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceOverride {
    pub layer: String,
    pub directory: String,
}

/// One source a reading used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceUse {
    /// The source's name; `None` for the unnamed local source.
    pub name: Option<String>,
    pub kind: SourceKind,
    /// The digest of the tree the reading read: a pin, an embedded
    /// build digest, or a computed content digest (for the local source,
    /// over the documents it read). `None` when the tree has none.
    pub identity: Option<String>,
    /// Whether the tree read is pinned: an origin with a pin, or an
    /// embedded source. Absent for the local source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<SourceOrigin>,
    pub overridden: bool,
    #[serde(rename = "override", skip_serializing_if = "Option::is_none")]
    pub replacement: Option<SourceOverride>,
}

impl SourceUse {
    pub fn new(name: Option<String>, kind: SourceKind, identity: Option<String>) -> Self {
        Self {
            name,
            kind,
            identity,
            pinned: None,
            origin: None,
            overridden: false,
            replacement: None,
        }
    }

    #[must_use]
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = Some(pinned);
        self
    }

    #[must_use]
    pub fn with_origin(mut self, origin: SourceOrigin) -> Self {
        self.origin = Some(origin);
        self
    }

    #[must_use]
    pub fn overridden_by(mut self, layer: impl Into<String>, directory: impl Into<String>) -> Self {
        self.overridden = true;
        self.replacement = Some(SourceOverride {
            layer: layer.into(),
            directory: directory.into(),
        });
        self
    }
}

/// The sources item: every source a reading used, the local source
/// first, then the named sources by name; and the source-set
/// fingerprint its cache and snapshot entries are keyed by, absent when
/// the reading used no named source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub sources: Vec<SourceUse>,
}

impl SourceSet {
    pub fn new(sources: Vec<SourceUse>, fingerprint: Option<String>) -> Self {
        Self {
            fingerprint,
            sources,
        }
    }
}
