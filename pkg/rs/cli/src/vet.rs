//! `kdlith vet dialect|schema`: load a definition with its imports,
//! compile and check it, and report every diagnostic on stderr. Success
//! prints one summary line on stdout.

use std::ffi::OsString;
use std::path::Path;

use kdlith_diagnostic::ExitStatus;
use kdlith_engine::{CheckScope, Definition, Extensions, load_dialect, load_schema};

use crate::report::report_all;

/// The usage line of the `vet` verb.
pub const VET_USAGE: &str = "kdlith vet dialect [--scope local|complete] <file>...\n       kdlith vet schema --dialect <file> [--dialect <file>]... [--scope local|complete] <file>...";

/// The definition level being vetted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VetTarget {
    Dialect,
    /// A schema definition, under a dialect loaded first.
    Schema,
}

/// Run `kdlith vet` over the arguments after the verb. A usage error
/// prints the usage line and exits 2.
pub fn vet_command(args: &[OsString]) -> ExitStatus {
    match parse(args) {
        Ok(parsed) => {
            let dialect: Vec<&Path> = parsed.dialect.iter().map(Path::new).collect();
            let entries: Vec<&Path> = parsed.entries.iter().map(Path::new).collect();
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
    entries: Vec<OsString>,
}

fn parse(args: &[OsString]) -> Result<VetArgs, String> {
    let (level, rest) = args.split_first().ok_or("vet needs dialect or schema")?;
    let target = match level.to_str() {
        Some("dialect") => VetTarget::Dialect,
        Some("schema") => VetTarget::Schema,
        _ => return Err(format!("unknown vet target {}", level.to_string_lossy())),
    };
    let mut parsed = VetArgs {
        target,
        scope: CheckScope::Complete,
        dialect: Vec::new(),
        entries: Vec::new(),
    };
    let mut scope_seen = false;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
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
            Some("--dialect") if target == VetTarget::Schema => {
                let file = iter.next().ok_or("--dialect needs a file")?;
                parsed.dialect.push(file.clone());
            }
            Some(flag) if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ => parsed.entries.push(arg.clone()),
        }
    }
    if parsed.entries.is_empty() {
        return Err("vet needs at least one definition document".to_owned());
    }
    if target == VetTarget::Schema && parsed.dialect.is_empty() {
        return Err("vet schema needs --dialect".to_owned());
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
