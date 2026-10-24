//! The `fx-*` helpers: the harness binary under other names,
//! dispatched on argv[0] and placed first on the processor's PATH.
//! See `populate_bin` for how the run's `bin/` directory is built.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::ExitCode;

/// The environment variables a run exports to the processor (and so to
/// any helper it starts), in report order.
pub(crate) const ENV_SUITE: &str = "KDLITH_FX_SUITE";
pub(crate) const ENV_KDLITH: &str = "KDLITH_FX_KDLITH";
pub(crate) const ENV_SCRATCH: &str = "KDLITH_FX_SCRATCH";
pub(crate) const ENV_SCENARIO: &str = "KDLITH_FX_SCENARIO";
/// The repository root, exported to the subprocesses of a codegen-oracle
/// run so a scenario's declared commands can resolve repository content.
pub(crate) const ENV_REPO: &str = "KDLITH_FX_REPO";

/// The helper that serves one kdlith invocation over embedded sources.
pub(crate) const EMBEDDED_HELPER: &str = "fx-kdlith-embedded";

/// Every helper name, for population.
const HELPERS: &[&str] = &["fx-env", EMBEDDED_HELPER];

/// Run the helper named by argv[0], if it names one.
pub(crate) fn dispatch_helper(argv0: &OsStr, argv: &[OsString]) -> Option<ExitCode> {
    let name = Path::new(argv0).file_name()?.to_str()?;
    match name {
        "fx-env" => Some(fx_env()),
        EMBEDDED_HELPER => Some(fx_kdlith_embedded(argv)),
        _ => None,
    }
}

/// `fx-kdlith-embedded NAME=DIR... -- ARGS...`: run the kdlith command
/// line ARGS through the CLI library with each DIR embedded as the source
/// NAME. A tree that cannot be loaded exits 3, which kdlith never uses.
fn fx_kdlith_embedded(argv: &[OsString]) -> ExitCode {
    let mut args = argv.iter().skip(1);
    let mut sources = Vec::new();
    for arg in args.by_ref() {
        if arg == "--" {
            break;
        }
        let loaded = arg
            .to_str()
            .and_then(|spec| spec.split_once('='))
            .ok_or_else(|| format!("source takes NAME=DIR, not {arg:?}"))
            .and_then(|(name, dir)| {
                embedded_files(Path::new(dir)).map(|files| (name.to_owned(), files))
            });
        match loaded {
            Ok((name, files)) => sources.push((name, kdlith_cli::SourceTree::embedded(files))),
            Err(message) => {
                eprintln!("{EMBEDDED_HELPER}: {message}");
                return ExitCode::from(3);
            }
        }
    }
    let command = std::iter::once(OsString::from("kdlith")).chain(args.cloned());
    kdlith_cli::run_with_sources(command, sources).into()
}

/// Every regular file under `root`, keyed by its `/`-separated path
/// relative to `root`. A link, a special file, or text that is not UTF-8
/// is refused, as the scenario copy refuses them.
fn embedded_files(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let read =
            std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for item in read {
            let item = item.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
            let path = item.path();
            let kind = item
                .file_type()
                .map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let key = path
                    .strip_prefix(root)
                    .ok()
                    .and_then(Path::to_str)
                    .ok_or_else(|| format!("{} has no UTF-8 key", path.display()))?
                    .to_owned();
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                files.push((key, text));
            } else {
                return Err(format!("{} is a link or special file", path.display()));
            }
        }
    }
    Ok(files)
}

/// `fx-env`: print the resolved suite, kdlith, scratch, and scenario
/// configuration as `key=value` lines. Fails when invoked outside a
/// harness run (any variable missing) instead of printing blanks.
fn fx_env() -> ExitCode {
    let mut missing = Vec::new();
    let mut lines = Vec::new();
    for (key, var) in [
        ("suite", ENV_SUITE),
        ("kdlith", ENV_KDLITH),
        ("scratch", ENV_SCRATCH),
        ("scenario", ENV_SCENARIO),
    ] {
        match std::env::var_os(var) {
            Some(v) if !v.is_empty() => lines.push(format!("{key}={}", v.to_string_lossy())),
            _ => missing.push(var),
        }
    }
    if !missing.is_empty() {
        eprintln!(
            "fx-env: not running under kdlith-fixtures (unset: {})",
            missing.join(", ")
        );
        return ExitCode::from(1);
    }
    for line in lines {
        println!("{line}");
    }
    ExitCode::SUCCESS
}

/// Populate a run's `bin/` with the harness under every helper name,
/// hard-linking, or copying across filesystems. Helpers never invoke
/// the processor under test.
pub(crate) fn populate_bin(bin_dir: &Path, harness: &Path) -> Result<(), String> {
    std::fs::create_dir_all(bin_dir)
        .map_err(|e| format!("cannot create {}: {e}", bin_dir.display()))?;
    for name in HELPERS {
        let target = bin_dir.join(name);
        if std::fs::hard_link(harness, &target).is_err() {
            std::fs::copy(harness, &target).map_err(|e| {
                format!(
                    "cannot link or copy {} to {}: {e}",
                    harness.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}
