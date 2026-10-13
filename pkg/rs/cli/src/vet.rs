//! `kdlith vet dialect|schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::ffi::OsString;
use std::path::Path;

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{
    BindingSession, CheckScope, Definition, Extensions, load_dialect, load_schema,
};
use kdlith_model::{BoundItem, SourceId};

use crate::report::report_all;

/// The usage line of the `vet` verb.
pub const VET_USAGE: &str = "kdlith vet dialect [--scope local|complete] <file>...\n       kdlith vet schema --dialect <file> [--dialect <file>]... [--scope local|complete] <file>...\n       kdlith vet data --dialect <file>... [--schema <file>]... <file>...";

/// The definition level being vetted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VetTarget {
    Dialect,
    /// A schema definition, under a dialect loaded first.
    Schema,
    /// Data documents, bound against a schema loaded under a dialect.
    Data,
}

/// Run `kdlith vet` over the arguments after the verb. A usage error
/// prints the usage line and exits 2.
pub fn vet_command(args: &[OsString]) -> ExitStatus {
    match parse(args) {
        Ok(parsed) => {
            let dialect: Vec<&Path> = parsed.dialect.iter().map(Path::new).collect();
            let entries: Vec<&Path> = parsed.entries.iter().map(Path::new).collect();
            if parsed.target == VetTarget::Data {
                let schema: Vec<&Path> = parsed.schema.iter().map(Path::new).collect();
                return vet_data(&dialect, &schema, &entries);
            }
            vet(parsed.target, &dialect, &entries, parsed.scope)
        }
        Err(reason) => {
            eprintln!("kdlith: {reason}\nusage: {VET_USAGE}");
            ExitStatus::Usage
        }
    }
}

struct VetArgs {
    target: VetTarget,
    scope: CheckScope,
    dialect: Vec<OsString>,
    schema: Vec<OsString>,
    entries: Vec<OsString>,
}

fn parse(args: &[OsString]) -> Result<VetArgs, String> {
    let (level, rest) = args.split_first().ok_or("vet needs dialect or schema")?;
    let target = match level.to_str() {
        Some("dialect") => VetTarget::Dialect,
        Some("schema") => VetTarget::Schema,
        Some("data") => VetTarget::Data,
        _ => return Err(format!("unknown vet target {}", level.to_string_lossy())),
    };
    let mut parsed = VetArgs {
        target,
        scope: CheckScope::Complete,
        dialect: Vec::new(),
        schema: Vec::new(),
        entries: Vec::new(),
    };
    let mut scope_seen = false;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--scope") if target == VetTarget::Data => {
                return Err("vet data checks complete scope only".to_owned());
            }
            Some("--scope") => {
                if scope_seen {
                    return Err("--scope given more than once".to_owned());
                }
                scope_seen = true;
                parsed.scope = match iter.next().and_then(|v| v.to_str()) {
                    Some("local") => CheckScope::Local,
                    Some("complete") => CheckScope::Complete,
                    _ => return Err("--scope takes local or complete".to_owned()),
                };
            }
            Some("--dialect") if target != VetTarget::Dialect => {
                let file = iter.next().ok_or("--dialect needs a file")?;
                parsed.dialect.push(file.clone());
            }
            Some("--schema") if target == VetTarget::Data => {
                let file = iter.next().ok_or("--schema needs a file")?;
                parsed.schema.push(file.clone());
            }
            Some(flag) if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ => parsed.entries.push(arg.clone()),
        }
    }
    if parsed.entries.is_empty() {
        return Err("vet needs at least one definition document".to_owned());
    }
    if target != VetTarget::Dialect && parsed.dialect.is_empty() {
        return Err("vet schema and vet data need --dialect".to_owned());
    }
    Ok(parsed)
}

/// Vet the definition whose entry documents are `entries`. A schema is
/// vetted under the dialect loaded from `dialect`, which is itself
/// vetted with complete checking first.
pub fn vet(
    target: VetTarget,
    dialect: &[&Path],
    entries: &[&Path],
    scope: CheckScope,
) -> ExitStatus {
    let extensions = Extensions::standard();
    let loaded = match target {
        VetTarget::Dialect => load_dialect(entries, &extensions, scope),
        VetTarget::Schema => match load_dialect(dialect, &extensions, CheckScope::Complete) {
            Ok(governing) => load_schema(&governing, entries, &extensions, scope),
            Err(records) => Err(records),
        },
        _ => {
            eprintln!("kdlith: vet data needs a schema; use vet_data\nusage: {VET_USAGE}");
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

/// Bind each data document against `schema` under `dialect`, or against
/// the dialect itself when `schema` is empty. Bound items go to stdout as
/// JSON lines; diagnostics also go to stderr.
pub fn vet_data(dialect: &[&Path], schema: &[&Path], entries: &[&Path]) -> ExitStatus {
    let extensions = Extensions::standard();
    let loaded = load_dialect(dialect, &extensions, CheckScope::Complete).and_then(|governing| {
        if schema.is_empty() {
            Ok(governing)
        } else {
            load_schema(&governing, schema, &extensions, CheckScope::Complete)
        }
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
fn read_text(path: &Path, shown: &str) -> Result<String, Box<DiagnosticRecord>> {
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
