//! `kdlith export`: thin orchestration over the engine's export. Data
//! goes to the `--format` stream; diagnostics and the completion
//! summary go to stderr only. Inputs come from the shared
//! [`DataOptions`].

use std::path::{Path, PathBuf};

use kdlith_diagnostic::ExitStatus;
use kdlith_engine::{Delivery, Emit, OutputPolicy, export_read};

use crate::report::report_all;
use crate::vet::DataOptions;
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
    let options = match args.profiles.data_options(args.anchors, args.handlers) {
        Ok(options) => options.with_dialect(args.dialect),
        Err(status) => return status,
    };
    export(
        &args.schema,
        &paths(&args.files),
        &emit,
        (delivery, policy),
        args.output.as_deref(),
        &options,
    )
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// Export the data documents under the schema: load the governing
/// dialect (the bundled one, or the one the data options name) and the
/// schema with complete checking under the data options' extensions, run the engine's export through a reader carrying the
/// options' handlers, under the options' anchors and profile, the
/// delivery, and the explicit output policy (`None` keeps the
/// fail-closed sensitivity defaults), and report. A failed run's
/// diagnostics decide the exit; a successful run writes its summary to
/// stderr so the data stream stays pure.
pub fn export(
    schema: &Path,
    entries: &[&Path],
    emit: &Emit,
    (delivery, policy): (Delivery, Option<OutputPolicy>),
    output: Option<&Path>,
    options: &DataOptions,
) -> ExitStatus {
    let extensions = options.extensions();
    let definition = match options.load_schema(schema, &extensions) {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let mut read_options = options.read_options();
    if let Some(policy) = policy {
        read_options = read_options.with_output_policy(policy);
    }
    let reader = options.reader(&definition);
    let outcome = export_read(&reader, entries, emit, delivery, output, &read_options);
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
