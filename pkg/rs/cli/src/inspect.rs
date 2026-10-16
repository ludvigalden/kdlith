//! `kdlith inspect dialect|schema|data`: read documents through the
//! reference processor and print each item as one JSON line (the source
//! view); diagnostics go to stderr.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::TypedItem;
use kdlith_superproc::{Item, reader};

use crate::InspectCommand;
use crate::ViewArg;
use crate::report::report_all;
use crate::vet::{definition_extensions, governing_dialect};

/// Run a parsed `inspect` subcommand. Every target shares the reading
/// surface; the target name records what kind of document the entries
/// are expected to hold. Only the source view exists; every other view
/// of the draft fails closed rather than printing a poorer stand-in.
pub(crate) fn run(command: InspectCommand) -> ExitStatus {
    match command {
        InspectCommand::Dialect(args) => {
            if let Some(status) = reject_view(args.view) {
                return status;
            }
            inspect(args.dialect.as_deref(), &paths(&args.files), &[])
        }
        InspectCommand::Schema(args) => {
            if let Some(status) = reject_view(args.view) {
                return status;
            }
            inspect(args.dialect.as_deref(), &paths(&args.files), &args.anchors)
        }
        InspectCommand::Data(args) => {
            if let Some(status) = reject_view(args.view) {
                return status;
            }
            match args.profiles.data_options(args.anchors, args.handlers) {
                Ok(options) => crate::vet::vet_data(
                    args.schema.as_deref(),
                    &paths(&args.files),
                    &options.with_dialect(args.dialect),
                ),
                Err(status) => status,
            }
        }
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// The exit of an unimplemented view, or `None` when the source view
/// was asked for.
fn reject_view(view: ViewArg) -> Option<ExitStatus> {
    (view != ViewArg::Source).then(|| {
        eprintln!("kdlith: the {} view is not implemented yet", view.name());
        ExitStatus::Failure
    })
}

/// Read `entries` and their imports as definition documents written in
/// the governing dialect — the bundled one, or the single document
/// `dialect` names — expanding the documents' import expressions
/// against `anchors`.
pub fn inspect(
    dialect: Option<&Path>,
    entries: &[&Path],
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    let extensions = definition_extensions();
    let compiled = match governing_dialect(dialect, &extensions) {
        Ok(compiled) => compiled,
        Err(records) => return report_all(&records),
    };
    let reader = match reader(&compiled) {
        Ok(reader) => reader.with_anchors(anchors),
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
