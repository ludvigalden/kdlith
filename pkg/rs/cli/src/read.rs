//! `kdlith read file <FILE>` (bare `kdlith read <FILE>` also accepted)
//! parses one document to its clean end; `read data` binds a data
//! document through a schema and prints the one record whose identity
//! was asked for.

use std::path::Path;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{Restored, record_identity};
use kdlith_model::Item;
use kdlith_parse::Parser;

use crate::DiagnosticsArg;
use crate::ReadArgs;
use crate::ReadScope;
use crate::report::{report, report_all};

/// Run a parsed `read` argument set: the explicit `file` and `data`
/// subcommands, or the bare one-argument compatibility form.
pub(crate) fn run(args: &ReadArgs) -> ExitStatus {
    match &args.command {
        Some(crate::ReadCommand::File(file)) => read(&file.file),
        Some(crate::ReadCommand::Data(data)) => read_data(data),
        None => match &args.file {
            Some(file) => read(file),
            None => {
                crate::usage::report_invalid_argument("read requires a document or a subcommand")
            }
        },
    }
}

/// Read `path` and drain its pull-event stream. Success prints nothing;
/// a failure prints one diagnostic line to stderr and fails with the
/// code's exit stratum.
pub fn read(path: &Path) -> ExitStatus {
    let shown = path.display().to_string();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return report(&source_error(
                "KDLITH-SRC001",
                &[&shown, &error.to_string()],
            ));
        }
    };
    let source = match std::str::from_utf8(&bytes) {
        Ok(source) => source,
        Err(error) => {
            let at = error.valid_up_to().to_string();
            return report(&source_error("KDLITH-SRC002", &[&shown, &at]));
        }
    };
    let mut parser = Parser::new(source);
    loop {
        match parser.next_event() {
            Ok(Some(_)) => {}
            Ok(None) => return ExitStatus::Success,
            Err(error) => return report(&error.to_diagnostic(source).with_source(shown)),
        }
    }
}

/// A source-area record with its registered template filled.
pub(crate) fn source_error(code: &str, values: &[&str]) -> DiagnosticRecord {
    let record = DiagnosticRecord::from_code(code).expect("source codes are registered and live");
    match lookup(code) {
        Some(definition) => record.with_message(render(definition.message_template, values)),
        None => record,
    }
}

/// Bind the data document (after the entry document, when one is
/// given) through the schema under the bundled dialect or the one
/// `--dialect` names, print the one record item whose identity was
/// asked for as a single JSON line, and report a missing identity as a
/// failure with a clear diagnostic. The
/// documents' imports execute as invocations, so a record reached
/// through an import is found like any other. `--scope` selects the
/// partition the lookup consults — every `(target, scope)` partition
/// the resolver settled under `complete` (the default), the scope-less
/// partition under `local`, only the named scope's partitions under a
/// scope name — while a record is found under each; it never gates the
/// exit. The binding always runs
/// to the end, so a diagnostic anywhere in the documents fails the read
/// even after the record is found. With `--state`, a snapshot of the
/// record restores its context and, of the read document, only its span
/// binds; a store that cannot serve it replays, and a snapshot from
/// another definition or entry chain fails (`KDLITH-CTX004`).
pub(crate) fn read_data(args: &crate::ReadDataArgs) -> ExitStatus {
    crate::report::mirror_diagnostics_as_json(args.diagnostics.is_some_and(DiagnosticsArg::json));
    let options = match args
        .profiles
        .clone()
        .data_options(args.locations.clone(), args.handlers.clone())
    {
        Ok(options) => options.with_dialect(args.dialect.clone()),
        Err(status) => return status,
    };
    let extensions = options.extensions();
    let definition = match options.load_schema(&args.schema, &extensions) {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let mut documents: Vec<&Path> = Vec::with_capacity(2);
    if let Some(entry) = args.entry.as_deref() {
        documents.push(entry);
    }
    documents.push(args.file.as_path());
    let reader = options.reader(&definition);
    let read_options = options.read_options();
    if let Some(state) = args.state.as_deref() {
        let mut found = Found::new(&args.id, &args.scope);
        let restored =
            reader.read_restored(&documents, &read_options, state, &args.id, &mut |item| {
                found.take(item)
            });
        match restored {
            Ok(Restored::Bound(_)) => return found.finish(args),
            Ok(Restored::Unusable(record)) => return report(&record),
            Ok(Restored::Mismatch(mismatch)) => {
                let (error, snapshot) = *mismatch;
                return report(&error.diagnostic(&snapshot));
            }
            Err(error) => {
                report_all(&found.records);
                eprintln!("kdlith: {error}");
                return ExitStatus::Failure;
            }
            // A miss emitted nothing: the documents are replayed.
            Ok(_) => {}
        }
    }
    let mut found = Found::new(&args.id, &args.scope);
    let read = reader.read(&documents, &read_options, &mut |item| found.take(item));
    if let Err(error) = read {
        report_all(&found.records);
        eprintln!("kdlith: {error}");
        return ExitStatus::Failure;
    }
    found.finish(args)
}

/// The diagnostics a reading reported and the first item carrying the
/// identity asked for under the scope the lookup consults.
struct Found<'a> {
    id: &'a str,
    scope: &'a ReadScope,
    records: Vec<DiagnosticRecord>,
    item: Option<Item>,
}

impl<'a> Found<'a> {
    fn new(id: &'a str, scope: &'a ReadScope) -> Self {
        Self {
            id,
            scope,
            records: Vec::new(),
            item: None,
        }
    }

    fn take(&mut self, item: Item) {
        if let Item::Diagnostic(record) = &item {
            self.records.push((**record).clone());
        }
        if self.item.is_none()
            && record_identity(&item) == Some(self.id)
            && admitted(self.scope, &item)
        {
            self.item = Some(item);
        }
    }

    /// Print the record found and every diagnostic, or report that none
    /// was found.
    fn finish(self, args: &crate::ReadDataArgs) -> ExitStatus {
        match self.item {
            Some(item) => {
                let mut out = std::io::stdout().lock();
                let written = write_item(&mut out, &item).is_ok();
                drop(out);
                if written {
                    report_all(&self.records)
                } else {
                    ExitStatus::Failure
                }
            }
            None => {
                report_all(&self.records);
                let scope = match self.scope {
                    ReadScope::Local => " under --scope local".to_owned(),
                    ReadScope::Complete => String::new(),
                    ReadScope::Named(name) => format!(" under --scope {name}"),
                };
                eprintln!(
                    "kdlith: no record with identity {} in {}{scope}",
                    args.id,
                    args.file.display()
                );
                ExitStatus::Failure
            }
        }
    }
}

/// Whether the lookup consults `item` under `scope`: a resolution is
/// consulted by complete scope, by local when scope-less, and by a name
/// when its partition carries that name; a record under every scope.
fn admitted(scope: &ReadScope, item: &Item) -> bool {
    match (scope, item) {
        (ReadScope::Local, Item::Resolution(resolved)) => resolved.scope.is_none(),
        (ReadScope::Named(name), Item::Resolution(resolved)) => {
            resolved.scope.as_deref() == Some(name.as_str())
        }
        _ => true,
    }
}

fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
