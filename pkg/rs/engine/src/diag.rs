//! Diagnostic construction for definition documents: registered codes,
//! rendered messages, located spans, and duplicate labels.

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Label};
use kdlith_parse::locate;

use crate::definition::DefinitionKind;
use crate::syntax::ParsedDocument;

pub(crate) const DUPLICATE_CONSTRUCT: &str = "KDLITH-DLC002";
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
pub(crate) const EXPANSION_CYCLE: &str = "KDLITH-SCH009";
pub(crate) const INSTANTIATION_CYCLE: &str = "KDLITH-SCH009";
pub(crate) const DISPATCH_PREFIX: &str = "KDLITH-SCH010";
pub(crate) const INHERITANCE_CYCLE: &str = "KDLITH-SCH012";
pub(crate) const SECOND_DEFAULT: &str = "KDLITH-SCH013";
pub(crate) const FALLBACK_IDENTITY: &str = "KDLITH-SCH014";
pub(crate) const EMPTY_NODE_GROUP: &str = "KDLITH-SCH015";
pub(crate) const INVALID_CONDITION_PROPERTY: &str = "KDLITH-SCH016";
pub(crate) const OVERLAPPING_WHEN_BRANCHES: &str = "KDLITH-SCH017";
pub(crate) const EMPTY_WHEN_COMPOSITION: &str = "KDLITH-SCH018";
pub(crate) const FALLBACK_LESS_WHEN_RULE: &str = "KDLITH-SCH019";
pub(crate) const IMPORT_CYCLE: &str = "KDLITH-IMP002";
pub(crate) const IMPORT_REPEATED: &str = "KDLITH-IMP003";
pub(crate) const LOCATION_UNRESOLVED: &str = "KDLITH-LOC001";
pub(crate) const LOCATION_OUTSIDE_SOURCE: &str = "KDLITH-LOC002";
pub(crate) const SOURCE_UNKNOWN: &str = "KDLITH-LOC003";
pub(crate) const LOCATION_NOT_ADMITTED: &str = "KDLITH-LOC004";
pub(crate) const SOURCE_INVALID: &str = "KDLITH-LOC005";
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
    notes: Vec<DiagnosticRecord>,
}

impl<'d> Diagnostics<'d> {
    pub(crate) fn new(documents: &'d [ParsedDocument], origin: &'static str) -> Self {
        Self {
            documents,
            origin,
            records: Vec::new(),
            notes: Vec::new(),
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

    /// A condition with an optional positioned note at another span of
    /// the same document.
    pub(crate) fn noted(
        &mut self,
        code: &'static str,
        site: Site,
        values: &[&str],
        note: Option<(ByteSpan, String)>,
    ) {
        let mut record = self.located(self.record(code, values), site);
        if let (Some((span, message)), Some(document)) = (note, self.documents.get(site.document)) {
            let at = locate(document.text(), span).start;
            let label = Label::new(Some(document.path().to_owned()), span, message)
                .with_position(at.line, at.column);
            record = record.with_label(label);
        }
        self.records.push(record);
    }

    pub(crate) fn has_errors(&self) -> bool {
        !self.records.is_empty()
    }

    /// An advisory condition: kept apart from the records, so it never
    /// fails the compilation.
    pub(crate) fn note(&mut self, code: &'static str, site: Site, values: &[&str]) {
        let record = self.located(self.record(code, values), site);
        self.notes.push(record);
    }

    pub(crate) fn take_notes(&mut self) -> Vec<DiagnosticRecord> {
        std::mem::take(&mut self.notes)
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

/// The note on an unknown node whose sibling written above it admits it
/// as a child: without braces, an indented line is still a sibling.
pub(crate) fn sibling_note(sibling: &str, child: &str) -> String {
    format!("the sibling {sibling} above admits {child} as a child — did you mean to nest it there")
}

/// The admitted names nearest to `given`, for a note naming what was
/// probably meant: Unicode-scalar edit distance within 2, ties broken by
/// the sorted name, never the name itself.
pub(crate) fn nearest_names<'a>(
    given: &str,
    admitted: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut found: Vec<(usize, String)> = admitted
        .into_iter()
        .filter_map(|name| {
            let distance = levenshtein(given, name);
            (distance > 0 && distance <= 2).then(|| (distance, name.to_owned()))
        })
        .collect();
    found.sort();
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|(_, name)| name).collect()
}

/// The note text naming the nearest admitted names, when any are close.
pub(crate) fn near_miss_note(candidates: &[String]) -> Option<String> {
    match candidates {
        [] => None,
        [one] => Some(format!("did you mean {one}?")),
        many => Some(format!("did you mean one of {}?", many.join(", "))),
    }
}

/// Levenshtein distance over Unicode scalars.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            next.push((row[j] + cost).min(row[j + 1] + 1).min(next[j] + 1));
        }
        row = next;
    }
    row[b.len()]
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
