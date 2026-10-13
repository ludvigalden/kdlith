//! `kdlith inspect dialect|schema`: read definition documents through
//! the reference processor and print each typed item as one JSON line
//! (the bound view); diagnostics go to stderr.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{CheckScope, Extensions, TypedItem};
use kdlith_superproc::{Item, load_dialect};

use crate::InspectCommand;
use crate::report::report_all;

/// Run a parsed `inspect` subcommand. Both targets share the reading
/// surface; the target name records what kind of definition the
/// documents are expected to hold.
pub(crate) fn run(command: InspectCommand) -> ExitStatus {
    let args = match command {
        InspectCommand::Dialect(args) | InspectCommand::Schema(args) => args,
    };
    let dialect: Vec<&Path> = args.dialect.iter().map(PathBuf::as_path).collect();
    let entries: Vec<&Path> = args.files.iter().map(PathBuf::as_path).collect();
    inspect(&dialect, &entries)
}

/// Read `entries` and their imports as definitions written in the
/// dialect loaded from `dialect`.
pub fn inspect(dialect: &[&Path], entries: &[&Path]) -> ExitStatus {
    let compiled = match load_dialect(dialect, &Extensions::standard(), CheckScope::Complete) {
        Ok(compiled) => compiled,
        Err(records) => return report_all(&records),
    };
    let reader = match kdlith_superproc::reader(&compiled) {
        Ok(reader) => reader,
        Err(records) => return report_all(&records),
    };
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    let mut written = true;
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Diagnostic(record) = &item {
            records.push((**record).clone());
        }
        written &= write_item(&mut out, &item).is_ok();
    };
    let read = reader.read(entries, &mut sink);
    drop(out);
    match read {
        Ok(_) if written => report_all(&records),
        Ok(_) => ExitStatus::Failure,
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            ExitStatus::Failure
        }
    }
}

fn write_item(out: &mut impl std::io::Write, item: &TypedItem<Item>) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
