use serde::Serialize;

/// Diagnostic severity classes. A severity is fixed once its code is
/// defined; a behavior change takes a new code, never a changed class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Half-open byte range over UTF-8 encoded source, file-relative. Byte
/// offsets are authoritative; a 1-based line and Unicode-scalar column
/// may ride beside the span, never instead of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ByteSpan {
    pub start: u32,
    pub end: u32,
}

impl ByteSpan {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// A secondary location attached to a diagnostic: another span that
/// explains the condition (the earlier declaration of a duplicate, the
/// import that brought a definition in, the binding a value came from).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Label {
    pub source_path: Option<String>,
    pub span: ByteSpan,
    /// The 1-based line and Unicode-scalar column of the span's start,
    /// when the label points at a line rather than a document.
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub message: String,
}

impl Label {
    pub fn new(source_path: Option<String>, span: ByteSpan, message: impl Into<String>) -> Self {
        Self {
            source_path,
            span,
            line: None,
            col: None,
            message: message.into(),
        }
    }

    /// Attach the line and column of the span's start.
    pub fn with_position(mut self, line: u32, col: u32) -> Self {
        self.line = Some(line);
        self.col = Some(col);
        self
    }
}

/// One diagnostic record. Records whose message needs values the
/// structured fields cannot express carry the filled template in
/// `message`, minted once by the detecting layer; the rest render at
/// presentation time from the registered template and the structured
/// fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DiagnosticRecord {
    pub code: String,
    pub severity: Severity,
    pub source_path: Option<String>,
    pub span: Option<ByteSpan>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    /// Provenance of the condition: the dialect definition, schema
    /// definition, or data document role it arose in.
    pub origin: Option<String>,
    pub labels: Vec<Label>,
    pub related: Vec<String>,
    pub correction: String,
    pub message: Option<String>,
}

impl From<Box<DiagnosticRecord>> for DiagnosticRecord {
    fn from(boxed: Box<DiagnosticRecord>) -> Self {
        *boxed
    }
}

impl DiagnosticRecord {
    /// Build a record for a registered code, taking the immutable
    /// severity and the correction from the code table. Returns `None`
    /// for a code outside every registered table or for a retired code:
    /// diagnostics never mint unregistered identities, and a retired
    /// code never mints again.
    pub fn from_code(code: &str) -> Option<Self> {
        let definition = crate::registry::lookup(code)?;
        if definition.retired {
            return None;
        }
        Some(Self {
            code: code.to_owned(),
            severity: definition.severity,
            source_path: None,
            span: None,
            line: None,
            col: None,
            origin: None,
            labels: Vec::new(),
            related: Vec::new(),
            correction: definition.correction.to_owned(),
            message: None,
        })
    }

    /// Attach the source document path.
    pub fn with_source(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    /// Attach the primary span, with its optional 1-based line and
    /// Unicode-scalar column.
    pub fn with_span(mut self, span: ByteSpan, line: Option<u32>, col: Option<u32>) -> Self {
        self.span = Some(span);
        self.line = line;
        self.col = col;
        self
    }

    /// Attach the provenance of the condition.
    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    /// Append a secondary label.
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Carry a message filled from the code's template at detection
    /// time; see [`crate::registry::render`].
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}
