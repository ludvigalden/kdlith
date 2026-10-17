//! `kdlith inspect dialect|schema|data`: read documents through the
//! reference processor and print each item as one JSON line (the source
//! view), then check a definition as `vet` does; diagnostics go to
//! stderr.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus, Severity};
use kdlith_engine::{CheckScope, Definition, Extensions, TypedItem};
use kdlith_superproc::{Item, load_dialect, load_schema_anchored, reader};

use crate::InspectCommand;
use crate::ViewArg;
use crate::report::report_all;
use crate::vet::{VetTarget, definition_extensions, governing_dialect};

/// Run a parsed `inspect` subcommand. Every target shares the reading
/// surface; the target name decides which definition kind the entries
/// are checked as. Only the source view exists; every other view of the
/// draft fails closed rather than printing a poorer stand-in.
pub(crate) fn run(command: InspectCommand) -> ExitStatus {
    match command {
        InspectCommand::Dialect(args) => {
            if let Some(status) = reject_view(args.view) {
                return status;
            }
            inspect(
                VetTarget::Dialect,
                args.dialect.as_deref(),
                &paths(&args.files),
                &[],
            )
        }
        InspectCommand::Schema(args) => {
            if let Some(status) = reject_view(args.view) {
                return status;
            }
            inspect(
                VetTarget::Schema,
                args.dialect.as_deref(),
                &paths(&args.files),
                &args.anchors,
            )
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
/// against `anchors`, then check them as the `target` definition with
/// the loader `vet` uses. Every item is shown before any compile fault
/// is reported: reader diagnostics, then compile diagnostics, decide the
/// exit together.
pub fn inspect(
    target: VetTarget,
    dialect: Option<&Path>,
    entries: &[&Path],
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    if target == VetTarget::Data {
        eprintln!("kdlith: inspect data binds data documents through vet data");
        return ExitStatus::Usage;
    }
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
        Ok(_) if written => {
            // The loader re-reads the same documents and stops on the
            // reader's own errors, so compiling after one would only
            // repeat them.
            if !records.iter().any(|r| r.severity == Severity::Error) {
                if let Err(compile) = check(target, &compiled, entries, &extensions, anchors) {
                    records.extend(compile);
                }
            }
            report_all(&records)
        }
        Ok(_) => ExitStatus::Failure,
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            ExitStatus::Failure
        }
    }
}

/// The compile stage: the same load `vet` runs on a dialect or schema
/// target, with complete checking.
fn check(
    target: VetTarget,
    governing: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    anchors: &[(String, PathBuf)],
) -> Result<Definition, Vec<DiagnosticRecord>> {
    match target {
        VetTarget::Schema => load_schema_anchored(
            governing,
            entries,
            extensions,
            CheckScope::Complete,
            anchors,
        ),
        _ => load_dialect(entries, extensions, CheckScope::Complete),
    }
}

fn write_item(out: &mut impl std::io::Write, item: &TypedItem<Item>) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
