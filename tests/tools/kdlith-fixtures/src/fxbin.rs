//! The `fx-*` helpers: the harness binary under other names, dispatched on
//! argv[0]. A run hard-links (or, across filesystems, copies) the harness
//! into its `bin/` directory under each helper name and puts that
//! directory first on the processor's PATH. Helpers never invoke the
//! processor under test.

use std::ffi::OsStr;
use std::path::Path;
use std::process::ExitCode;

/// The environment variables a run exports to the processor (and so to
/// any helper it starts), in report order.
pub(crate) const ENV_SUITE: &str = "KDLITH_FX_SUITE";
pub(crate) const ENV_KDLITH: &str = "KDLITH_FX_KDLITH";
pub(crate) const ENV_SCRATCH: &str = "KDLITH_FX_SCRATCH";
pub(crate) const ENV_SCENARIO: &str = "KDLITH_FX_SCENARIO";

/// Every helper name, for population.
const HELPERS: &[&str] = &["fx-env"];

/// Run the helper named by argv[0], if it names one.
pub(crate) fn dispatch_helper(argv0: &OsStr) -> Option<ExitCode> {
    let name = Path::new(argv0).file_name()?.to_str()?;
    match name {
        "fx-env" => Some(fx_env()),
        _ => None,
    }
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

/// Populate a run's `bin/` with the harness under every helper name.
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
