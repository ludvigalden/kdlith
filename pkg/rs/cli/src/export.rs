//! `kdlith export`: thin orchestration over the engine's export. Data
//! goes to the `--format` stream; diagnostics and the completion
//! summary go to stderr only.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::ExitStatus;
use kdlith_engine::{CheckScope, Delivery, Emit, Extensions, OutputPolicy, export_under};
use kdlith_superproc::{load_schema, seed};

use crate::report::report_all;
use crate::{DeliveryArg, ExportArgs, SensitiveArg};

/// Run a parsed `export` argument set.
pub(crate) fn run(args: ExportArgs) -> ExitStatus {
    let emit = if args.emit == "roots" {
        Emit::Roots
    } else {
        Emit::Type(args.emit.clone())
    };
    let delivery = match args.delivery {
        DeliveryArg::Atomic => Delivery::Atomic,
        DeliveryArg::Provisional => Delivery::Provisional,
    };
    let policy = args.sensitive.map(|sensitive| match sensitive {
        SensitiveArg::Emit => OutputPolicy::Emit,
        SensitiveArg::Omit => OutputPolicy::Omit,
        SensitiveArg::Redact => OutputPolicy::Redact,
    });
    export(
        &args.schema,
        &paths(&args.files),
        &emit,
        (delivery, policy),
        args.output.as_deref(),
    )
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// Export the data documents under the schema: load the bundled dialect
/// and the schema with complete checking, run the engine's export under
/// the delivery and the explicit output policy (`None` keeps the
/// fail-closed sensitivity defaults), and report. A failed run's
/// diagnostics decide the exit; a successful run writes its summary to
/// stderr so the data stream stays pure.
pub fn export(
    schema: &Path,
    entries: &[&Path],
    emit: &Emit,
    (delivery, policy): (Delivery, Option<OutputPolicy>),
    output: Option<&Path>,
) -> ExitStatus {
    let extensions = Extensions::standard();
    let governing = match seed(&extensions) {
        Ok(governing) => governing,
        Err(records) => return report_all(&records),
    };
    let definition = match load_schema(&governing, &[schema], &extensions, CheckScope::Complete) {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let outcome = export_under(&definition, entries, emit, delivery, output, policy);
    let status = report_all(&outcome.records);
    if outcome.rejected.is_some() {
        return ExitStatus::Failure;
    }
    if let Some(broken) = &outcome.broken {
        eprintln!("kdlith: {broken}");
        return ExitStatus::Failure;
    }
    if status == ExitStatus::Success {
        let dest = output
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "stdout".to_owned());
        eprintln!("kdlith: exported {} items to {dest}", outcome.emitted);
    }
    status
}
