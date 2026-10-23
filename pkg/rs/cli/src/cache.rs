//! `kdlith cache`: thin orchestration over the store and the engine's
//! derivation. Entry lines on stdout; diagnostics and summaries on
//! stderr. Every form names its state directory explicitly.

use std::fmt::Display;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{CacheScan, clean_state, derive_state, scan_state, write_state};

use crate::report::{report, report_all};
use crate::{CacheCommand, CacheRefreshArgs};

/// Run a parsed `cache` subcommand.
pub(crate) fn run(command: CacheCommand) -> ExitStatus {
    match command {
        CacheCommand::Inspect(args) => inspect(&args.state),
        CacheCommand::Refresh(args) => refresh(*args),
        CacheCommand::Clean(args) => clean(&args.state),
    }
}

/// One line per intact entry, led by its class (`cache` or `snapshot`),
/// then each corrupt or foreign entry reported. A missing state
/// directory is empty, not an error.
fn inspect(state: &Path) -> ExitStatus {
    let scan = match scan_state(state) {
        Ok(scan) => scan,
        Err(record) => return report(&record),
    };
    if !scan.present {
        eprintln!(
            "kdlith: state directory {} does not exist; nothing is cached",
            state.display()
        );
        return ExitStatus::Success;
    }
    let printed = print_lines(&scan.entries);
    combine(printed, report_all(&scan.records(state)))
}

/// Derive the state the data reading caches — bound results and record
/// context snapshots — and replace the owned entries with it. Nothing
/// is written when the reading fails or the directory holds a foreign
/// entry.
fn refresh(args: CacheRefreshArgs) -> ExitStatus {
    let scan = match scan_state(&args.state) {
        Ok(scan) => scan,
        Err(record) => return report(&record),
    };
    if !scan.foreign.is_empty() {
        let records: Vec<DiagnosticRecord> = scan.records(&args.state);
        let status = report_all(&records);
        eprintln!(
            "kdlith: refusing to write {}: it holds entries the cache does not own",
            args.state.display()
        );
        return combine(ExitStatus::Failure, status);
    }
    let options = match args.profiles.data_options(args.anchors, args.handlers) {
        Ok(options) => options.with_dialect(args.dialect),
        Err(status) => return status,
    };
    let extensions = options.extensions();
    let def = match options.load_schema(&args.schema, &extensions) {
        Ok(def) => def,
        Err(records) => return report_all(&records),
    };
    let entries: Vec<&Path> = args.files.iter().map(PathBuf::as_path).collect();
    let reader = options.reader(&def);
    let derived = match derive_state(&reader, &entries, &options.read_options()) {
        Ok(derived) => derived,
        Err(error) => {
            eprintln!("kdlith: {error}");
            return ExitStatus::Failure;
        }
    };
    let status = report_all(&derived.records);
    if derived.failed {
        eprintln!(
            "kdlith: the reading failed; {} was left unchanged",
            args.state.display()
        );
        return combine(ExitStatus::Failure, status);
    }
    let outcome = match write_state(&args.state, &scan, &derived.entries) {
        Ok(outcome) => outcome,
        Err(record) => return report(&record),
    };
    let lines: Vec<_> = derived.entries.iter().map(|e| e.entry.clone()).collect();
    let printed = print_lines(&lines);
    eprintln!(
        "kdlith: refreshed {} entries in {}; removed {} stale",
        outcome.written.len(),
        args.state.display(),
        outcome.removed.len()
    );
    combine(printed, status)
}

/// Remove every owned entry of either class; leave and report the rest.
/// A scan that cannot cover the whole directory removes nothing. Every
/// owned removal is attempted; each failure is reported and fails the
/// run.
fn clean(state: &Path) -> ExitStatus {
    let scan = match scan_state(state) {
        Ok(scan) => scan,
        Err(record) => return report(&record),
    };
    if !scan.present {
        eprintln!(
            "kdlith: state directory {} does not exist; nothing to clean",
            state.display()
        );
        return ExitStatus::Success;
    }
    let outcome = clean_state(state, &scan);
    let lines: Vec<String> = outcome
        .removed
        .iter()
        .map(|name| format!("removed {name}"))
        .collect();
    let printed = print_lines(&lines);
    let mut records = outcome.failures.clone();
    records.extend(foreign_only(&scan, state));
    let status = report_all(&records);
    let unremoved = if outcome.is_complete() {
        String::new()
    } else {
        format!("; could not remove {}", outcome.failures.len())
    };
    eprintln!(
        "kdlith: removed {} owned entries from {}; left {} foreign{unremoved}",
        outcome.removed.len(),
        state.display(),
        scan.foreign.len()
    );
    combine(printed, status)
}

/// The foreign-entry records of a scan: after a cleaning every corrupt
/// owned entry is gone, so only the foreign ones remain to report.
fn foreign_only(scan: &CacheScan, state: &Path) -> Vec<DiagnosticRecord> {
    let mut only = scan.clone();
    only.corrupt.clear();
    only.records(state)
}

/// The first failing status of two, else success.
fn combine(first: ExitStatus, second: ExitStatus) -> ExitStatus {
    if first == ExitStatus::Success {
        second
    } else {
        first
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
