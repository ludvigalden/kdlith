//! The export driver: selects `roots` or one record type from the
//! execution-backed data path and delivers it through the runtime
//! staging writer; callers load the definition and map the outcome.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::{Item, SourceId};
use kdlith_parse::{Event, Parser};
use kdlith_runtime::{Delivery, ExportWriter, SourceRevisions, export_line};

use crate::documents::read_text;
use crate::link::{LinkInputs, link_reading};
use crate::{CheckScope, DataReader, Definition, OutputPolicy, ReadOptions, TypeKind};

const ORIGIN: &str = "export";
const INCOMPLETE: &str = "KDLITH-EXP002";
const UNSELECTABLE: &str = "KDLITH-EXP003";

/// What an export run selects from the processed stream.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Emit {
    /// The bound items of the syntactic top-level nodes of the
    /// explicitly listed entry documents. Imported roots are never
    /// silently included: only a document named on the invocation
    /// contributes.
    Roots,
    /// The records of one named type, across the whole processed import
    /// graph.
    Type(String),
}

/// The result of one export run: every diagnostic to report, how many
/// items were emitted, and — when the run never read a document — why.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ExportOutcome {
    /// Every diagnostic of the run in detection order, including any
    /// `KDLITH-EXP001` write failure and the `KDLITH-EXP002`
    /// incomplete-stream warning.
    pub records: Vec<DiagnosticRecord>,
    /// How many selected items were written to the destination.
    pub emitted: usize,
    /// The run never read a document: the type selection named no
    /// record type of the definition. The reason; its `KDLITH-EXP003`
    /// record is among `records`.
    pub rejected: Option<String>,
    /// The reading's link diagnostics under local scope: reported, but
    /// never deciding the run's failure. Under complete scope the link
    /// diagnostics are among `records` instead and fail the run.
    pub ungated: Vec<DiagnosticRecord>,
    /// The reading broke its own contract; an engine error, not a
    /// document diagnostic.
    pub broken: Option<String>,
}

/// Run one export under the fail-closed sensitivity defaults: a secret
/// field is redacted, a reference field emitted. See [`export_under`].
pub fn export(
    def: &Definition,
    entries: &[&Path],
    emit: &Emit,
    delivery: Delivery,
    output: Option<&Path>,
) -> ExportOutcome {
    export_under(def, entries, emit, delivery, output, None)
}

/// Run one export: read `entries` — and the documents they import —
/// under `def`, select per `emit`, and deliver NDJSON lines to `output`
/// (or stdout when `None`) under `delivery`. Each line is the item with
/// its provenance envelope beside its data: the node's location, the
/// source revisions every location names, one shared origin table, and
/// each emitted field's origin in it. `policy` is the explicit output
/// policy every sensitive field passes; `None` keeps each sensitivity's
/// fail-closed default, so a secret is emitted only under an explicit
/// `emit`. Error diagnostics in the documents, a link diagnostic of the
/// reading's complete-scope link composition, a broken reading, or an
/// unwritable destination fail the run. Under atomic delivery a failing
/// run publishes nothing; under provisional delivery a failing run that
/// already emitted lines keeps them and carries the `KDLITH-EXP002`
/// warning naming the count, while the failing exit itself comes from
/// the processing failure's error diagnostics.
pub fn export_under(
    def: &Definition,
    entries: &[&Path],
    emit: &Emit,
    delivery: Delivery,
    output: Option<&Path>,
    policy: Option<OutputPolicy>,
) -> ExportOutcome {
    let options = match policy {
        Some(policy) => ReadOptions::new().with_output_policy(policy),
        None => ReadOptions::new(),
    };
    export_read(
        &DataReader::new(def),
        entries,
        emit,
        delivery,
        output,
        &options,
        CheckScope::Complete,
    )
}

/// Run one export as [`export_under`] does, through a configured
/// reader and its reading options: the reader carries the context
/// handlers the processor installed, and `options` the profile, the
/// root and sources, and the explicit output policy. The type
/// selection is checked against the reader's definition before any
/// document is read. After the stream the reading is linked under
/// `scope`'s complete arm — the same one link composition `vet data`
/// runs — so under complete scope a link diagnostic fails the run like
/// any processing failure; under local scope the link diagnostics land
/// in `ungated` and never decide the run.
pub fn export_read(
    reader: &DataReader<'_>,
    entries: &[&Path],
    emit: &Emit,
    delivery: Delivery,
    output: Option<&Path>,
    options: &ReadOptions,
    scope: CheckScope,
) -> ExportOutcome {
    let def = reader.definition();
    if let Emit::Type(name) = emit
        && let Some(rejection) = reject_type(def, name)
    {
        return ExportOutcome {
            records: vec![unselectable(name, &rejection)],
            rejected: Some(rejection),
            ..ExportOutcome::default()
        };
    }
    let pending = match emit {
        Emit::Roots => entry_roots(entries),
        Emit::Type(_) => HashMap::new(),
    };
    let mut writer = match ExportWriter::open(delivery, output) {
        Ok(writer) => writer,
        Err(record) => {
            return ExportOutcome {
                records: vec![*record],
                ..ExportOutcome::default()
            };
        }
    };
    let mut selection = Selection {
        emit,
        pending,
        roots: HashMap::new(),
        current: None,
    };
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut write_failed = false;
    let mut revisions = SourceRevisions::new();
    let mut inputs = LinkInputs::new();
    let read = {
        let mut sink = |item: Item| {
            inputs.observe(&item);
            revisions.observe(&item);
            match &item {
                // Each entry's subtree closes with its own document-wide
                // completion: a marker outside any entry opens the entry it
                // names; every marker inside one is an import.
                Item::Document(document) => {
                    if selection.current.is_none()
                        && let Some(roots) = selection.pending.remove(&document.path)
                    {
                        selection.roots.insert(document.source, roots);
                        selection.current = Some(document.source);
                    }
                    return;
                }
                Item::ScopeComplete(completion) if completion.scope.is_none() => {
                    if selection.current == Some(completion.source) {
                        selection.current = None;
                    }
                    return;
                }
                Item::Diagnostic(record) => {
                    records.push((**record).clone());
                    return;
                }
                _ => {}
            }
            if !write_failed && selection.selected(&item) {
                let line = export_line(&item, revisions.provenance(&item).as_ref());
                match writer.write_item(&line) {
                    Ok(()) => {}
                    Err(record) => {
                        records.push(*record);
                        write_failed = true;
                    }
                }
            }
        };
        reader.read(entries, options, &mut sink)
    };
    let mut outcome = ExportOutcome {
        records,
        emitted: writer.emitted(),
        ..ExportOutcome::default()
    };
    let processing_failed = match read {
        Ok(failed) => {
            let report = link_reading(def, &inputs);
            if scope == CheckScope::Complete {
                let failed = failed || report.has_errors();
                outcome.records.extend(report.diagnostics.iter().cloned());
                failed
            } else {
                outcome.ungated = report.diagnostics;
                failed
            }
        }
        Err(error) => {
            outcome.broken = Some(error.to_string());
            true
        }
    };
    if write_failed || processing_failed {
        writer.discard();
        if processing_failed && delivery == Delivery::Provisional && outcome.emitted > 0 {
            outcome.records.push(incomplete(outcome.emitted));
        }
        return outcome;
    }
    match writer.commit() {
        Ok(()) => outcome,
        Err(record) => {
            outcome.records.push(*record);
            outcome
        }
    }
}

/// The selection state one export builds as the stream arrives: which
/// entry the stream is inside, and the top-level node indices of every
/// entry seen so far.
struct Selection<'a> {
    emit: &'a Emit,
    /// The top-level node start offsets of each entry document, by the
    /// path as the invocation wrote it, waiting for the source identity
    /// the reading assigns it.
    pending: HashMap<String, HashSet<u32>>,
    /// Top-level node start offsets by entry source identity.
    roots: HashMap<SourceId, HashSet<u32>>,
    /// The entry whose subtree the stream is inside; `None` between
    /// entries.
    current: Option<SourceId>,
}

impl Selection<'_> {
    fn selected(&self, item: &Item) -> bool {
        match self.emit {
            Emit::Roots => match item {
                Item::Record(record) => self.is_root(record.node.source, record.span),
                Item::Declaration(declaration) => {
                    self.is_root(declaration.node.source, declaration.span)
                }
                Item::Contribution(contribution) => {
                    self.is_root(contribution.node.source, contribution.span)
                }
                _ => false,
            },
            Emit::Type(name) => {
                matches!(item, Item::Record(record) if record.type_name == name.as_str())
            }
        }
    }

    /// Whether a bound item is one of its entry's top-level nodes: the
    /// item's node opened at a start offset the entry's syntax scan
    /// recorded at depth zero. Offsets, not node numbers, decide it, so
    /// a subtree the binder skipped never shifts the match.
    fn is_root(&self, source: SourceId, span: ByteSpan) -> bool {
        self.roots
            .get(&source)
            .is_some_and(|roots: &HashSet<u32>| roots.contains(&span.start))
    }
}

/// A type selection must name a type the definition declares as an
/// object: records only ever bind to object types, so anything else
/// would emit nothing while appearing to have selected something.
fn reject_type(def: &Definition, name: &str) -> Option<String> {
    let declared = def
        .names
        .get(name)
        .and_then(|sym| def.types.lookup(sym))
        .and_then(|id| def.types.get(id));
    match declared.map(|decl| &decl.kind) {
        Some(TypeKind::Object(_)) => None,
        Some(_) => Some(format!("type {name} is not a record type")),
        None => Some(format!("no type {name} in the schema")),
    }
}

/// The start offsets of the syntactic top-level nodes of each entry
/// document, by the path as the invocation wrote it. A document that
/// cannot be read contributes nothing; the reading reports the read
/// failure itself.
fn entry_roots(entries: &[&Path]) -> HashMap<String, HashSet<u32>> {
    entries
        .iter()
        .map(|entry| {
            let shown = entry.display().to_string();
            let roots = match read_text(entry, &shown, "document size", ORIGIN) {
                Ok(text) => top_level_starts(&text),
                Err(_) => HashSet::new(),
            };
            (shown, roots)
        })
        .collect()
}

/// The start offsets of a document's depth-0 node events: the same
/// parser events, and so the same spans, the binder opens its frames
/// at. A parse error stops the scan; the reading reports the error.
fn top_level_starts(text: &str) -> HashSet<u32> {
    let mut roots = HashSet::new();
    let mut parser = Parser::new(text);
    let mut depth = 0u32;
    while let Ok(Some(event)) = parser.next_event() {
        match event.item {
            Event::Node { .. } => {
                if depth == 0 {
                    roots.insert(event.span.start);
                }
                depth += 1;
            }
            Event::End => depth = depth.saturating_sub(1),
            // Entries and children blocks open no node.
            _ => {}
        }
    }
    roots
}

/// The `KDLITH-EXP003` record for a type selection that names no record
/// type of the definition.
fn unselectable(name: &str, reason: &str) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(UNSELECTABLE)
        .expect("KDLITH-EXP003 is registered and never retired");
    match lookup(UNSELECTABLE) {
        Some(definition) => {
            record.with_message(render(definition.message_template, &[name, reason]))
        }
        None => record,
    }
    .with_origin(ORIGIN)
}

/// The `KDLITH-EXP002` warning naming a provisional stream's emitted
/// count. The failing exit comes from the processing failure's own
/// diagnostics; this warning never moves it.
fn incomplete(count: usize) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(INCOMPLETE)
        .expect("KDLITH-EXP002 is registered and never retired");
    match lookup(INCOMPLETE) {
        Some(definition) => {
            record.with_message(render(definition.message_template, &[&count.to_string()]))
        }
        None => record,
    }
    .with_origin(ORIGIN)
}
