//! `kdlith gen dialect|schema`: generate a processor crate from
//! definition documents through the codegen library. The verb owns
//! the invocation surface; emission and the ownership guards
//! belong to `kdlith_codegen`.

use std::path::{Path, PathBuf};

use kdlith_codegen as codegen;
use kdlith_codegen::WriteError;
use kdlith_diagnostic::ExitStatus;
use kdlith_engine::Extensions;

use crate::GenCommand;
use crate::report::report_all;

/// Run a parsed `gen` subcommand.
pub(crate) fn run(command: GenCommand) -> ExitStatus {
    match command {
        GenCommand::Dialect(args) => generate(
            GenTarget::Dialect,
            &paths(&args.files),
            (&args.crate_name, &args.out),
            mode_of(args.dry_run, args.check),
            &[],
        ),
        GenCommand::Schema(args) => generate(
            GenTarget::Schema,
            &paths(&args.form.files),
            (&args.form.crate_name, &args.form.out),
            mode_of(args.form.dry_run, args.form.check),
            &args.anchors,
        ),
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// The mode flags decide what the run does with the generated tree;
/// clap has already refused `--dry-run --check` together.
fn mode_of(dry_run: bool, check: bool) -> GenMode {
    match (dry_run, check) {
        (true, _) => GenMode::DryRun,
        (false, true) => GenMode::Check,
        (false, false) => GenMode::Write,
    }
}

/// The definition form being generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenTarget {
    /// A dialect definition: the generated crate embeds its documents
    /// as the seed of a self-hosting processor.
    Dialect,
    /// A schema definition: the generated crate is a data reader that
    /// loads the schema through the reference processor.
    Schema,
}

/// What a generation run does with the generated tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenMode {
    /// Write the tree under the output root, through the ownership
    /// guard.
    Write,
    /// Print the generator-owned paths and write nothing.
    DryRun,
    /// Compare against the output root without writing; any difference
    /// reports `KDLITH-GEN002` and fails the run.
    Check,
}

/// Generate the processor crate the definition documents compile to and
/// write it under `out` — or, in the dry-run mode, print the plan, or,
/// in the check mode, compare without writing. Loading failures report
/// their diagnostics on stderr; a write into an unowned path refuses
/// with `KDLITH-GEN001`, and check drift reports `KDLITH-GEN002`, both
/// through the standard diagnostic surface. A schema's own import
/// expressions expand against `anchors`; a dialect root binds no
/// discovery grammar, so a dialect's imports are literal and take none.
pub fn generate(
    target: GenTarget,
    entries: &[&Path],
    (crate_name, out): (&str, &Path),
    mode: GenMode,
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    let form = match target {
        GenTarget::Dialect => "dialect",
        GenTarget::Schema => "schema",
    };
    let extensions = Extensions::standard();
    let tree = match target {
        GenTarget::Dialect => codegen::generate_dialect(entries, &extensions, crate_name),
        GenTarget::Schema => codegen::generate_schema(entries, &extensions, crate_name, anchors),
    };
    let tree = match tree {
        Ok(tree) => tree,
        Err(codegen::Error::DefinitionRejected { records }) => return report_all(&records),
        Err(error) => return failed(form, &error),
    };
    match mode {
        GenMode::DryRun => {
            println!(
                "plan: {form} crate {crate_name} -> {} ({} owned files, dry run)",
                out.display(),
                tree.file_count()
            );
            for path in tree.paths() {
                println!("  {path}");
            }
            ExitStatus::Success
        }
        GenMode::Check => match codegen::check(&tree, out) {
            Ok(findings) if findings.is_empty() => {
                println!(
                    "ok: {form} crate {crate_name} at {} is current ({} owned files)",
                    out.display(),
                    tree.file_count()
                );
                ExitStatus::Success
            }
            Ok(findings) => report_all(&findings),
            Err(error) => failed(form, &error),
        },
        GenMode::Write => match codegen::write(&tree, out) {
            Ok(()) => {
                println!(
                    "ok: generated {form} crate {crate_name} at {} ({} owned files)",
                    out.display(),
                    tree.file_count()
                );
                ExitStatus::Success
            }
            Err(WriteError::Refused(records)) => report_all(&records),
            Err(error) => failed(form, &error),
        },
    }
}

/// Report a matchable library error under the verb's context; the exit
/// is the general failure stratum.
fn failed(form: &str, error: &dyn std::fmt::Display) -> ExitStatus {
    eprintln!("kdlith: gen {form}: {error}");
    ExitStatus::Failure
}
