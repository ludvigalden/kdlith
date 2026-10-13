//! `kdlith vet dialect|schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{BindingSession, CheckScope, Definition, Extensions};
use kdlith_model::{BoundItem, SourceId};
use kdlith_superproc::{load_dialect, load_schema, seed};

use crate::report::report_all;
use crate::{ScopeArg, VetCommand};

/// Run a parsed `vet` subcommand.
pub(crate) fn run(command: VetCommand) -> ExitStatus {
    match command {
        VetCommand::Dialect(args) => vet(
            VetTarget::Dialect,
            None,
            &paths(&args.files),
            scope_of(args.scope),
        ),
        VetCommand::Schema(args) => vet(
            VetTarget::Schema,
            args.dialect.as_deref(),
            &paths(&args.files),
            scope_of(args.scope),
        ),
        VetCommand::Data(args) => vet_data(
            args.dialect.as_deref(),
            args.schema.as_deref(),
            &paths(&args.files),
        ),
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

fn scope_of(scope: ScopeArg) -> CheckScope {
    match scope {
        ScopeArg::Local => CheckScope::Local,
        ScopeArg::Complete => CheckScope::Complete,
    }
}

/// The governing dialect: the bundled canonical dialect, or the one
/// entry document an explicit `--dialect` names, itself loaded with
/// complete checking.
pub(crate) fn governing_dialect(
    dialect: Option<&Path>,
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    match dialect {
        Some(file) => load_dialect(&[file], extensions, CheckScope::Complete),
        None => seed(extensions),
    }
}

/// The definition level being vetted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VetTarget {
    Dialect,
    /// A schema definition, under the bundled dialect or the one given
    /// by an explicit `--dialect`.
    Schema,
    /// Data documents, bound against a schema loaded under a dialect.
    Data,
}

/// Vet the definition whose entry documents are `entries`. A schema is
/// vetted under the bundled dialect, or the dialect loaded from the one
/// document `dialect` names, which is itself vetted with complete
/// checking first.
pub fn vet(
    target: VetTarget,
    dialect: Option<&Path>,
    entries: &[&Path],
    scope: CheckScope,
) -> ExitStatus {
    let extensions = Extensions::standard();
    let loaded = match target {
        VetTarget::Dialect => load_dialect(entries, &extensions, scope),
        VetTarget::Schema => match governing_dialect(dialect, &extensions) {
            Ok(governing) => load_schema(&governing, entries, &extensions, scope),
            Err(records) => Err(records),
        },
        VetTarget::Data => {
            eprintln!("kdlith: vet data binds data documents through vet data");
            return ExitStatus::Usage;
        }
    };
    match loaded {
        Ok(definition) => {
            println!("{}", summary_line(&definition));
            ExitStatus::Success
        }
        Err(records) => report_all(&records),
    }
}

fn summary_line(definition: &Definition) -> String {
    let declared = definition
        .types
        .iter()
        .filter(|(_, decl)| decl.site.is_some())
        .count();
    format!(
        "ok: {} {} version {} scope={} documents={} types={declared} nodes={} slots={} deferrals={} digest={}",
        definition.kind.root_name(),
        definition.name,
        definition.version.as_deref().unwrap_or("-"),
        definition.scope.name(),
        definition.documents.len(),
        definition.bindings.len(),
        definition.slots.len(),
        definition.deferrals.len(),
        definition.digest(),
    )
}

/// Bind each data document against `schema` under the governing
/// dialect — the bundled one, or the single document `dialect` names —
/// or against the dialect itself when `schema` is absent. Bound items
/// go to stdout as JSON lines; diagnostics go to stderr.
pub fn vet_data(dialect: Option<&Path>, schema: Option<&Path>, entries: &[&Path]) -> ExitStatus {
    let extensions = Extensions::standard();
    let loaded = governing_dialect(dialect, &extensions).and_then(|governing| match schema {
        None => Ok(governing),
        Some(file) => load_schema(&governing, &[file], &extensions, CheckScope::Complete),
    });
    let definition = match loaded {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    for (at, entry) in entries.iter().enumerate() {
        let shown = entry.display().to_string();
        let text = match read_text(entry, &shown) {
            Ok(text) => text,
            Err(record) => {
                records.push(*record);
                continue;
            }
        };
        let source = SourceId(u32::try_from(at).unwrap_or(u32::MAX));
        let mut session = BindingSession::new(&definition, source, &shown, &text);
        loop {
            match session.next_item() {
                Ok(Some(item)) => {
                    if let BoundItem::Diagnostic(record) = &item {
                        records.push((**record).clone());
                    }
                    if write_item(&mut out, &item).is_err() {
                        return ExitStatus::Failure;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    eprintln!("kdlith: {shown}: {error}");
                    return ExitStatus::Failure;
                }
            }
        }
    }
    drop(out);
    report_all(&records)
}

fn write_item(out: &mut impl std::io::Write, item: &BoundItem) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}

/// A data document's text, or the registered source diagnostic.
pub(crate) fn read_text(path: &Path, shown: &str) -> Result<String, Box<DiagnosticRecord>> {
    let bytes = std::fs::read(path).map_err(|error| {
        let reason = error.to_string();
        Box::new(crate::read::source_error(
            "KDLITH-SRC001",
            &[shown, &reason],
        ))
    })?;
    String::from_utf8(bytes).map_err(|error| {
        let at = error.utf8_error().valid_up_to().to_string();
        Box::new(crate::read::source_error("KDLITH-SRC002", &[shown, &at]))
    })
}
