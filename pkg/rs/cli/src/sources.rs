//! `kdlith sources plan|verify|list`: report the sources a manifest
//! declares, one JSON line per source on stdout; diagnostics on stderr.
//! Nothing is fetched.

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{ReadOptions, SourcesReport, list_sources, plan_sources, verify_sources};

use crate::inspect::write_lines;
use crate::report::report_all;
use crate::usage;
use crate::vet::DataOptions;
use crate::{SourcesCommand, SourcesVerbArgs};

type Report = fn(&ReadOptions) -> Result<SourcesReport, Vec<DiagnosticRecord>>;

/// Run a parsed `sources` subcommand. `plan` and `verify` report the
/// origins of a manifest, so they need `--sources` (`KDLITH-CLI001`).
pub(crate) fn run(command: SourcesCommand) -> ExitStatus {
    let (args, report, needs_manifest): (SourcesVerbArgs, Report, bool) = match command {
        SourcesCommand::Plan(args) => (args, plan_sources, true),
        SourcesCommand::Verify(args) => (args, verify_sources, true),
        SourcesCommand::List(args) => (args, list_sources, false),
    };
    if needs_manifest && args.locations.manifest.is_none() {
        return usage::report_invalid_argument(
            "the sources plan and verify forms need --sources FILE",
        );
    }
    let options = args
        .locations
        .apply(DataOptions::new(None, Vec::new()))
        .read_options();
    match report(&options) {
        Ok(SourcesReport { lines, records, .. }) => {
            let printed = write_lines(&lines);
            let status = report_all(&records);
            match printed {
                ExitStatus::Success => status,
                failed => failed,
            }
        }
        Err(records) => report_all(&records),
    }
}
