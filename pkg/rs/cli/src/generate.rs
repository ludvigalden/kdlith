//! `kdlith gen dialect|schema|workspace`: the invocation surface over
//! the codegen library, which owns emission, the workspace checks,
//! and the ownership guards.

use std::path::{Path, PathBuf};

use kdlith_codegen as codegen;
use kdlith_codegen::{Packaging, WriteError};
use kdlith_diagnostic::{DiagnosticRecord, ExitStatus, Severity};
use kdlith_engine::{DeclaredSource, Extensions, ReadOptions};

use crate::handlers::registry;
use crate::report::report_all;
use crate::usage;
use crate::vet::definition_extensions;
use crate::{GenCommand, GenWorkspaceArgs};

/// Run a parsed `gen` subcommand.
pub(crate) fn run(command: GenCommand) -> ExitStatus {
    match command {
        GenCommand::Dialect(args) => generate(
            GenTarget::Dialect,
            &paths(&args.files),
            (&args.crate_name, &args.out),
            mode_of(args.dry_run, args.check),
            ReadOptions::new(),
            &definition_extensions(),
            packaging_of(args.kdlith_path, args.kdlith_version, args.kdlith_workspace),
        ),
        GenCommand::Workspace(args) => workspace(&args),
        // Generation admits the named handlers' names and never runs
        // them: the generated processor installs its own.
        GenCommand::Schema(args) => {
            let args = *args;
            generate(
                GenTarget::Schema,
                &paths(&args.form.files),
                (&args.form.crate_name, &args.form.out),
                mode_of(args.form.dry_run, args.form.check),
                args.locations.read_options(),
                &definition_extensions().with_installed_handlers(&registry(&args.handlers)),
                packaging_of(
                    args.form.kdlith_path,
                    args.form.kdlith_version,
                    args.form.kdlith_workspace,
                ),
            )
        }
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// The packaging flags decide how the manifest names the kdlith
/// crates; clap has already refused any two together.
fn packaging_of(path: Option<String>, version: Option<String>, workspace: bool) -> Packaging {
    match (path, version) {
        (Some(root), _) => Packaging::Path(root),
        (None, Some(version)) => Packaging::Registry(version),
        (None, None) if workspace => Packaging::Workspace,
        (None, None) => Packaging::Sibling,
    }
}

/// `gen workspace`: check the root manifest's kdlith dependencies
/// against the named source (`KDLITH-GEN006`, never written), and
/// write, plan, or check the owned patch file.
fn workspace(args: &GenWorkspaceArgs) -> ExitStatus {
    if args.locations.manifest.is_none() {
        return usage::report_invalid_argument("gen workspace needs --sources FILE");
    }
    let name = args.kdlith_source.as_str();
    let options = args.locations.read_options();
    let (declared, mut records) = match codegen_source(&options, name) {
        Ok(found) => found,
        Err(records) => return report_all(&records),
    };
    let source = match codegen::cargo_source(name, &declared) {
        Ok(source) => source,
        Err(refusal) => {
            records.push(*refusal);
            return report_all(&records);
        }
    };
    let root = args.locations.root().unwrap_or_default();
    let checkout = declared.overridden.as_deref();
    let patch = match codegen::cargo_patch(&source, checkout) {
        Ok(patch) => patch,
        Err(error) => return failed("workspace", &error),
    };
    let config = root.join(codegen::CARGO_CONFIG);
    let patched = match checkout {
        Some(dir) => format!(
            "{} patched crates from {}",
            codegen::patch_crates().count(),
            dir.display()
        ),
        None => "marker only, no override".to_owned(),
    };
    let lines = codegen::workspace_dependencies(&source);
    if args.dry_run {
        print_dependencies(&root, name, &lines);
        println!("plan: {} ({patched}, dry run)", config.display());
        return report_all(&records);
    }
    match codegen::check_workspace(&root, &source) {
        Ok(findings) => records.extend(findings),
        Err(error) => return failed("workspace", &error),
    }
    if args.check {
        match codegen::check_patch(&root, &patch) {
            Ok(findings) => records.extend(findings),
            Err(error) => return failed("workspace", &error),
        }
        if !records.iter().any(|r| r.severity == Severity::Error) {
            println!(
                "ok: workspace {} is current for source {name} ({} dependencies, {patched})",
                shown_root(&root),
                lines.len()
            );
        }
        return report_all(&records);
    }
    print_dependencies(&root, name, &lines);
    match codegen::write_patch(&root, &patch) {
        Ok(()) => {
            println!("ok: wrote {} ({patched})", config.display());
            let seed_from = checkout.map_or_else(|| PathBuf::from("<checkout>"), Path::to_path_buf);
            println!(
                "note: with an empty cargo git cache and an unreachable origin, seed it once from a local checkout: {}",
                codegen::seed_recipe(&source, &seed_from)
            );
        }
        Err(WriteError::Refused(refusals)) => records.extend(refusals),
        Err(error) => return failed("workspace", &error),
    }
    report_all(&records)
}

/// The named source as the sources manifest and overrides declare it.
fn codegen_source(
    options: &ReadOptions,
    name: &str,
) -> Result<(DeclaredSource, Vec<DiagnosticRecord>), Vec<DiagnosticRecord>> {
    kdlith_engine::declared_source(options, name, "pkg/rs")
}

fn shown_root(root: &Path) -> String {
    if root.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        root.display().to_string()
    }
}

/// The expected root dependency lines, each ready to paste.
fn print_dependencies(root: &Path, name: &str, lines: &[String]) {
    println!(
        "[workspace.dependencies] of {} for source {name}:",
        root.join("Cargo.toml").display()
    );
    for line in lines {
        println!("{line}");
    }
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
/// their diagnostics on stderr, and every refusal of the generator
/// prints as one `kdlith: gen <form>:` line; a write into an unowned
/// path refuses with `KDLITH-GEN001`, and check drift reports
/// `KDLITH-GEN002`, both through the standard diagnostic surface. A
/// schema's own import locations resolve under `options`; a dialect's
/// imports are literal locations, so it takes none. The definition
/// loads under `extensions`, and the manifest names the kdlith crates by
/// `packaging`.
pub fn generate(
    target: GenTarget,
    entries: &[&Path],
    (crate_name, out): (&str, &Path),
    mode: GenMode,
    options: ReadOptions,
    extensions: &Extensions,
    packaging: Packaging,
) -> ExitStatus {
    let form = match target {
        GenTarget::Dialect => "dialect",
        GenTarget::Schema => "schema",
    };
    let tree = match target {
        GenTarget::Dialect => codegen::generate_dialect(entries, extensions, crate_name, packaging),
        GenTarget::Schema => {
            codegen::generate_schema(entries, extensions, crate_name, options, packaging)
        }
    };
    let tree = match tree {
        Ok(tree) => tree,
        Err(codegen::Error::DefinitionRejected { records }) => return report_all(&records),
        Err(error) => return failed(form, &error),
    };
    let noted = report_all(tree.notes());
    if noted != ExitStatus::Success {
        return noted;
    }
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
