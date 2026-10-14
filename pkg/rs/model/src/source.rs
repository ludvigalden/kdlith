use kdlith_diagnostic::ByteSpan;
use serde::{Deserialize, Serialize};

/// Session-local handle for one source revision. Stable within a
/// session; persisted data refers to the descriptor, not the handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u32);

/// The role a source plays, in the framework's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DocumentRole {
    DialectDefinition,
    SchemaDefinition,
    DataDocument,
}

/// One source revision. Spans are only reliable provenance when tied to
/// the content digest of the revision they were measured against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SourceDescriptor {
    pub id: SourceId,
    pub path: String,
    pub role: DocumentRole,
    /// Content digest of the revision, stamped with its algorithm.
    pub digest: Option<String>,
}

impl SourceDescriptor {
    pub fn new(id: SourceId, path: impl Into<String>, role: DocumentRole) -> Self {
        Self {
            id,
            path: path.into(),
            role,
            digest: None,
        }
    }
}

/// A point in UTF-8 source. The byte offset is authoritative; the 1-based
/// line and 1-based Unicode-scalar column ride beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Position {
    pub offset: u32,
    pub line: u32,
    pub column: u32,
}

impl Position {
    pub fn new(offset: u32, line: u32, column: u32) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }
}

/// Half-open range between two positions in one source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// The byte range alone, in the form diagnostics carry.
    pub fn byte_span(self) -> ByteSpan {
        ByteSpan::new(self.start.offset, self.end.offset)
    }
}

/// A span within a specific source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

impl Location {
    pub fn new(source: SourceId, span: Span) -> Self {
        Self { source, span }
    }

    /// The location moved to `to` when it lies in `from`.
    pub fn relocate(&mut self, from: SourceId, to: SourceId) {
        if self.source == from {
            self.source = to;
        }
    }
}
