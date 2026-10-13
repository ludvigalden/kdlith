//! Diagnostic construction for definition documents: registered codes,
//! rendered messages, located spans, and duplicate labels.

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Label};
use kdlith_parse::locate;

use crate::definition::DefinitionKind;
use crate::syntax::ParsedDocument;

pub(crate) const DUPLICATE_CONSTRUCT: &str = "KDLITH-DLC002";
pub(crate) const DEFERRED_MECHANISM: &str = "KDLITH-DLC003";
pub(crate) const ROOT_MISMATCH: &str = "KDLITH-DLC004";
pub(crate) const MALFORMED: &str = "KDLITH-DLC005";
pub(crate) const NOT_INSTALLED: &str = "KDLITH-DLC006";
pub(crate) const TYPE_EXPRESSION: &str = "KDLITH-DLC007";
pub(crate) const DUPLICATE_TYPE: &str = "KDLITH-SCH002";
pub(crate) const CONFLICTING_BINDING: &str = "KDLITH-SCH003";
pub(crate) const DUPLICATE_DECLARATION: &str = "KDLITH-SCH004";
pub(crate) const INVALID_BINDING: &str = "KDLITH-SCH005";
pub(crate) const DUPLICATE_PROPERTY: &str = "KDLITH-SYN003";
pub(crate) const UNRESOLVED_DIALECT: &str = "KDLITH-DLC008";
pub(crate) const UNKNOWN_TYPE: &str = "KDLITH-SCH001";
pub(crate) const UNRESOLVED_SCHEMA: &str = "KDLITH-SCH006";
pub(crate) const NOT_STREAMABLE: &str = "KDLITH-SCH007";
pub(crate) const DIALECT_LITERAL: &str = "KDLITH-DLC009";
pub(crate) const SCHEMA_LITERAL: &str = "KDLITH-SCH008";
pub(crate) const IMPORT_UNRESOLVED: &str = "KDLITH-IMP001";
pub(crate) const IMPORT_CYCLE: &str = "KDLITH-IMP002";
pub(crate) const IMPORT_REPEATED: &str = "KDLITH-IMP003";
pub(crate) const SOURCE_UNREADABLE: &str = "KDLITH-SRC001";
pub(crate) const SOURCE_NOT_UTF8: &str = "KDLITH-SRC002";
pub(crate) const SOURCE_LIMIT: &str = "KDLITH-SRC003";

// Binding codes. A definition is a data document of its dialect, so a
// construct the dialect rejects gets these codes on every reading path.
pub(crate) const UNKNOWN_NODE: &str = "KDLITH-BND001";
pub(crate) const CARDINALITY: &str = "KDLITH-BND002";
pub(crate) const VALUE_TYPE: &str = "KDLITH-BND003";
pub(crate) const MISSING_FIELD: &str = "KDLITH-BND004";
pub(crate) const UNBOUND_ENTRY: &str = "KDLITH-BND005";
pub(crate) const DUPLICATE_KEY: &str = "KDLITH-BND006";
pub(crate) const UNKNOWN_MEMBER: &str = "KDLITH-BND008";

/// Where a construct was written: the contributing document's position
/// in the compiler input and the byte span of the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Site {
    pub document: usize,
    pub span: ByteSpan,
}

impl Site {
    pub(crate) fn new(document: usize, span: ByteSpan) -> Self {
        Self { document, span }
    }
}

/// The diagnostics of one compilation, in detection order.
pub(crate) struct Diagnostics<'d> {
    documents: &'d [ParsedDocument],
    origin: &'static str,
    records: Vec<DiagnosticRecord>,
}

impl<'d> Diagnostics<'d> {
    pub(crate) fn new(documents: &'d [ParsedDocument], origin: &'static str) -> Self {
        Self {
            documents,
            origin,
            records: Vec::new(),
        }
    }

    pub(crate) fn error(&mut self, code: &'static str, site: Site, values: &[&str]) {
        let record = self.located(self.record(code, values), site);
        self.records.push(record);
    }

    /// A condition with no position in any document.
    pub(crate) fn unsited(&mut self, code: &'static str, values: &[&str]) {
        let record = self.record(code, values);
        self.records.push(record);
    }

    /// A condition with a secondary label at an earlier site.
    pub(crate) fn labelled(
        &mut self,
        code: &'static str,
        site: Site,
        earlier: Site,
        values: &[&str],
        note: &str,
    ) {
        let path = self
            .documents
            .get(earlier.document)
            .map(|document| document.path().to_owned());
        let record = self
            .located(self.record(code, values), site)
            .with_label(Label::new(path, earlier.span, note));
        self.records.push(record);
    }

    pub(crate) fn has_errors(&self) -> bool {
        !self.records.is_empty()
    }

    /// The number of diagnostics so far, to tell whether a step added one.
    pub(crate) fn count(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn into_records(self) -> Vec<DiagnosticRecord> {
        self.records
    }

    fn record(&self, code: &'static str, values: &[&str]) -> DiagnosticRecord {
        record(code, values, self.origin)
    }

    fn located(&self, record: DiagnosticRecord, site: Site) -> DiagnosticRecord {
        match self.documents.get(site.document) {
            Some(document) => located(record, document, site.span),
            None => record,
        }
    }
}

/// A registered code with its message rendered and its origin set.
pub(crate) fn record(code: &'static str, values: &[&str], origin: &str) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(code)
        .expect("engine diagnostic codes are registered and never retired");
    let record = match lookup(code) {
        Some(definition) => record.with_message(render(definition.message_template, values)),
        None => record,
    };
    record.with_origin(origin)
}

/// A record placed at a span of a parsed document.
pub(crate) fn located(
    record: DiagnosticRecord,
    document: &ParsedDocument,
    span: ByteSpan,
) -> DiagnosticRecord {
    let start = locate(document.text(), span).start;
    record
        .with_source(document.path())
        .with_span(span, Some(start.line), Some(start.column))
}

/// Report a key repeated in a keyed collection of a construct, such as
/// two fields of one object: the dialect binds these collections with
/// `duplicate-keys reject`, so this is the binding code on every path.
pub(crate) fn duplicate_key(diags: &mut Diagnostics<'_>, sites: (Site, Site), key: &str, of: &str) {
    diags.labelled(
        DUPLICATE_KEY,
        sites.0,
        sites.1,
        &[key, of],
        "first given here",
    );
}

/// Report a declaration made twice: a dialect construct, a schema type,
/// or another schema declaration, labelled with the first declaration.
pub(crate) fn duplicate(
    diags: &mut Diagnostics<'_>,
    kind: DefinitionKind,
    sites: (Site, Site),
    what: &str,
    name: &str,
) {
    let subject = format!("{what} {name}");
    let (code, value) = match kind {
        DefinitionKind::Dialect => (DUPLICATE_CONSTRUCT, subject.as_str()),
        DefinitionKind::Schema if what == "type" => (DUPLICATE_TYPE, name),
        DefinitionKind::Schema => (DUPLICATE_DECLARATION, subject.as_str()),
    };
    diags.labelled(code, sites.0, sites.1, &[value], "first declared here");
}
