//! `kdlith inspect dialect|schema`: read definition documents through
//! the reference processor and print each typed item as one JSON line
//! (the bound view); diagnostics go to stderr.

use std::ffi::OsString;
use std::path::Path;

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{CheckScope, Extensions, TypedItem, load_dialect};
use kdlith_superproc::Item;

use crate::report::report_all;

/// The usage line of the `inspect` verb.
pub const INSPECT_USAGE: &str = "kdlith inspect dialect|schema --dialect <file>... <file>...";

/// Run `kdlith inspect` over the arguments after the verb. A usage error
/// prints the usage line and exits 2.
pub fn inspect_command(args: &[OsString]) -> ExitStatus {
    match parse(args) {
        Ok((dialect, entries)) => {
            let dialect: Vec<&Path> = dialect.iter().map(Path::new).collect();
            let entries: Vec<&Path> = entries.iter().map(Path::new).collect();
            inspect(&dialect, &entries)
        }
        Err(reason) => {
            eprintln!("kdlith: {reason}\nusage: {INSPECT_USAGE}");
            ExitStatus::Usage
        }
    }
}

type Parsed = (Vec<OsString>, Vec<OsString>);

fn parse(args: &[OsString]) -> Result<Parsed, String> {
    let (level, rest) = args
        .split_first()
        .ok_or("inspect needs dialect or schema")?;
    if !matches!(level.to_str(), Some("dialect" | "schema")) {
        return Err(format!(
            "unknown inspect target {}",
            level.to_string_lossy()
        ));
    }
    let (mut dialect, mut entries) = (Vec::new(), Vec::new());
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--dialect") => dialect.push(iter.next().ok_or("--dialect needs a file")?.clone()),
            Some(flag) if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ => entries.push(arg.clone()),
        }
    }
    if dialect.is_empty() {
        return Err("inspect needs --dialect".to_owned());
    }
    if entries.is_empty() {
        return Err("inspect needs at least one definition document".to_owned());
    }
    Ok((dialect, entries))
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
