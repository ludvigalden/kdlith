use serde::Serialize;

use crate::record::DiagnosticRecord;

/// Envelope codec version, fixed by the interface. A consumer reading an
/// unknown version must fail; no envelope is ever emitted unversioned.
pub const ENVELOPE_VERSION: u8 = 1;

/// The result class of a completed command response. The kind/payload
/// pairing is fixed per command; a command never changes kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ResultKind {
    /// A verdict: validation reports and check results.
    Result,
    /// Enumerated items: declared types, nodes, profiles, indexed items.
    Listing,
    /// One definition, data item, or explanation view.
    Content,
    /// Expected filesystem changes, e.g. a generation dry run.
    Plan,
    /// Derived-state status, e.g. cache inspection.
    Status,
}

/// Who produced the envelope: the producing binary's version.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct OriginRecord {
    pub binary: String,
}

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct PaginationRecord {
    pub pages: Vec<u32>,
    pub page_size: u32,
    pub total_page_count: u32,
}

impl PaginationRecord {
    pub fn new(pages: Vec<u32>, page_size: u32, total_page_count: u32) -> Self {
        Self {
            pages,
            page_size,
            total_page_count,
        }
    }
}

/// One character-limit cut: Unicode-scalar extents with the cut target's
/// identity, so extents are honest in every encoding.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct TruncationRecord {
    pub target: String,
    pub emitted_scalars: u64,
    pub total_scalars: u64,
}

impl TruncationRecord {
    pub fn new(target: impl Into<String>, emitted_scalars: u64, total_scalars: u64) -> Self {
        Self {
            target: target.into(),
            emitted_scalars,
            total_scalars,
        }
    }
}

/// An informational envelope note. Notes never fail a command.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct NoteRecord {
    pub code: String,
    pub message: String,
}

impl NoteRecord {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// What a command actually covered. `complete: false` is honest
/// coverage, never by itself a failing verdict: the exit follows the
/// diagnostics, while the record names what the result does and does
/// not cover.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CoverageRecord {
    /// The explicitly selected entrypoint documents.
    pub entrypoints: Vec<String>,
    /// Documents reached through imports from the entrypoints.
    pub imports: Vec<String>,
    /// Selectors applied to the processed graph (`--emit`, `--type`,
    /// `--id`, ...), in invocation order.
    pub selectors: Vec<String>,
    pub complete: bool,
    /// Documents the run could not read or process, with the reason.
    pub gaps: Vec<String>,
}

impl CoverageRecord {
    pub fn new(entrypoints: Vec<String>, selectors: Vec<String>, complete: bool) -> Self {
        Self {
            entrypoints,
            imports: Vec::new(),
            selectors,
            complete,
            gaps: Vec::new(),
        }
    }

    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }

    pub fn with_gaps(mut self, gaps: Vec<String>) -> Self {
        self.gaps = gaps;
        self
    }
}

/// The versioned command-response envelope. Every completed command
/// response is one envelope; every stdout encoding carries the same
/// information — a record class one encoding cannot carry is a defect
/// in the envelope, never a license to drop it. Exported data is not a
/// command response and never travels inside an envelope.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Envelope {
    pub v: u8,
    pub kind: ResultKind,
    /// Command path, argv[0] omitted, e.g. `["gen", "schema"]`.
    pub command: Vec<String>,
    pub origin: OriginRecord,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub pagination: Option<PaginationRecord>,
    pub coverage: Option<CoverageRecord>,
    pub truncations: Vec<TruncationRecord>,
    pub notes: Vec<NoteRecord>,
    pub result: serde_json::Value,
}

impl Envelope {
    pub fn new(kind: ResultKind, command: Vec<String>) -> Self {
        Self {
            v: ENVELOPE_VERSION,
            kind,
            command,
            origin: OriginRecord {
                binary: env!("CARGO_PKG_VERSION").to_owned(),
            },
            diagnostics: Vec::new(),
            pagination: None,
            coverage: None,
            truncations: Vec::new(),
            notes: Vec::new(),
            result: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    pub fn push_diagnostic(&mut self, diagnostic: DiagnosticRecord) {
        self.diagnostics.push(diagnostic);
    }

    pub fn push_note(&mut self, code: &str, message: impl Into<String>) {
        self.notes.push(NoteRecord::new(code, message));
    }
}
