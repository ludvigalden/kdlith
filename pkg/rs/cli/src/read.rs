//! `kdlith read file <FILE>` (bare `kdlith read <FILE>` accepted as a
//! compatibility alias) parses one document to its clean end; `read
//! data` binds a data document through a schema and prints the one
//! record whose identity was asked for.

use std::path::Path;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{CheckScope, DataReader};
use kdlith_model::{Item, Value};
use kdlith_parse::Parser;
use kdlith_superproc::{load_schema, seed};

use crate::ReadArgs;
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
                eprintln!("kdlith: read requires a document or a subcommand");
                ExitStatus::Usage
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
/// given) through the schema under the bundled dialect, print the one
/// record item whose identity was asked for as a single JSON line, and
/// report a missing identity as a failure with a clear diagnostic. The
/// documents' imports execute as invocations, so a record reached
/// through an import is found like any other. The binding always runs
/// to the end, so a diagnostic anywhere in the documents fails the read
/// even after the record is found.
pub(crate) fn read_data(args: &crate::ReadDataArgs) -> ExitStatus {
    let options = crate::vet::DataOptions::new(
        args.anchors.clone(),
        args.profile.clone(),
        args.handlers.clone(),
    );
    let extensions = options.extensions();
    let governing = match seed(&extensions) {
        Ok(governing) => governing,
        Err(records) => return report_all(&records),
    };
    let definition = match load_schema(
        &governing,
        &[args.schema.as_path()],
        &extensions,
        CheckScope::Complete,
    ) {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let mut documents: Vec<&Path> = Vec::with_capacity(2);
    if let Some(entry) = args.entry.as_deref() {
        documents.push(entry);
    }
    documents.push(args.file.as_path());
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut found: Option<Item> = None;
    let mut sink = |item: Item| {
        if let Item::Diagnostic(record) = &item {
            records.push((**record).clone());
        }
        if found.is_none() && identity_of(&item) == Some(args.id.as_str()) {
            found = Some(item);
        }
    };
    let reader = DataReader::with_handlers(&definition, options.registry());
    let read = reader.read(&documents, &options.read_options(), &mut sink);
    if let Err(error) = read {
        report_all(&records);
        eprintln!("kdlith: {error}");
        return ExitStatus::Failure;
    }
    match found {
        Some(item) => {
            let mut out = std::io::stdout().lock();
            let written = write_item(&mut out, &item).is_ok();
            drop(out);
            if written {
                report_all(&records)
            } else {
                ExitStatus::Failure
            }
        }
        None => {
            report_all(&records);
            eprintln!(
                "kdlith: no record with identity {} in {}",
                args.id,
                args.file.display()
            );
            ExitStatus::Failure
        }
    }
}

/// A record item's lookup identity: the string value of its bound
/// `id` field. Records carry no registered identity of their own, and
/// the schemas read here bind their record identity from that field.
fn identity_of(item: &Item) -> Option<&str> {
    let Item::Record(record) = item else {
        return None;
    };
    record
        .fields
        .iter()
        .find(|field| field.name == "id")
        .and_then(|field| match &field.value {
            Value::String(id) => Some(id.as_str()),
            _ => None,
        })
}

fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
