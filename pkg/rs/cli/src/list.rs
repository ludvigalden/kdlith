//! `kdlith list`: thin orchestration over the engine's enumeration.
//! One stdout line per entry in sorted order; diagnostics on stderr.

use std::fmt::Display;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::ExitStatus;
use kdlith_engine::{
    DataReader, Definition, Extensions, index_items, list_nodes, list_profiles, list_types,
};

use crate::report::report_all;
use crate::vet::DataOptions;
use crate::{ListCommand, ListItemsArgs, ListSchemaArgs};

/// Run a parsed `list` subcommand.
pub(crate) fn run(command: ListCommand) -> ExitStatus {
    match command {
        ListCommand::Types(args) => match load_listed(&args) {
            Ok(def) => print_lines(&list_types(&def)),
            Err(status) => status,
        },
        ListCommand::Nodes(args) => match load_listed(&args) {
            Ok(def) => print_lines(&list_nodes(&def)),
            Err(status) => status,
        },
        ListCommand::Profiles(args) => match load_listed(&args) {
            Ok(def) => print_lines(&list_profiles(&def)),
            Err(status) => status,
        },
        ListCommand::Items(args) => items(args),
    }
}

/// The schema a declaration listing names, under the standard
/// extensions, the governing dialect, and the anchors it supplies.
fn load_listed(args: &ListSchemaArgs) -> Result<Definition, ExitStatus> {
    let options =
        DataOptions::new(args.anchors.clone(), None, Vec::new()).with_dialect(args.dialect.clone());
    load(&args.schema, &options, &Extensions::standard())
}

/// The schema loaded under the options' governing dialect with complete
/// checking, its import expressions expanded against the options'
/// anchors; a failed load is reported and decides the exit.
fn load(
    schema: &Path,
    options: &DataOptions,
    extensions: &Extensions,
) -> Result<Definition, ExitStatus> {
    options
        .load_schema(schema, extensions)
        .map_err(|records| report_all(&records))
}

/// Index the data documents through the schema and print one line per
/// indexed item. The reading runs to its end, so a diagnostic anywhere
/// in the documents fails the listing after the index is printed.
fn items(args: ListItemsArgs) -> ExitStatus {
    let options = match args.profiles.data_options(args.anchors, args.handlers) {
        Ok(options) => options.with_dialect(args.dialect),
        Err(status) => return status,
    };
    let def = match load(&args.schema, &options, &options.extensions()) {
        Ok(def) => def,
        Err(status) => return status,
    };
    let entries: Vec<&Path> = args.files.iter().map(PathBuf::as_path).collect();
    let reader = DataReader::with_handlers(&def, options.registry());
    match index_items(&reader, &entries, &options.read_options()) {
        Ok(index) => {
            let printed = print_lines(&index.entries);
            let reported = report_all(&index.records);
            if printed == ExitStatus::Success {
                reported
            } else {
                printed
            }
        }
        Err(error) => {
            eprintln!("kdlith: {error}");
            ExitStatus::Failure
        }
    }
}

/// Print each entry as one stdout line; a failed write fails the run.
fn print_lines<T: Display>(entries: &[T]) -> ExitStatus {
    let mut out = std::io::stdout().lock();
    for entry in entries {
        if writeln!(out, "{entry}").is_err() {
            return ExitStatus::Failure;
        }
    }
    if out.flush().is_err() {
        return ExitStatus::Failure;
    }
    ExitStatus::Success
}
