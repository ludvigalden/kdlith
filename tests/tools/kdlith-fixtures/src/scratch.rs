//! Scratch materialization: one run directory per invocation, one
//! copied scenario directory per trial, and no-follow teardown with
//! leftovers recorded loudly.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cleanrun;

/// The per-invocation scratch area.
#[derive(Debug)]
pub(crate) struct RunDir {
    pub(crate) root: PathBuf,
}

impl RunDir {
    /// Create `<scratch root>/<run id>` (and the scratch root itself when
    /// absent). The run directory must not already exist.
    pub(crate) fn create(scratch_root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(scratch_root)
            .map_err(|e| format!("cannot create scratch root {}: {e}", scratch_root.display()))?;
        let scratch_root = std::fs::canonicalize(scratch_root).map_err(|e| {
            format!(
                "cannot resolve scratch root {}: {e}",
                scratch_root.display()
            )
        })?;
        let root = scratch_root.join(run_id());
        std::fs::create_dir(&root)
            .map_err(|e| format!("cannot create run directory {}: {e}", root.display()))?;
        Ok(Self { root })
    }

    /// The helper directory placed first on the processor's PATH.
    pub(crate) fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// Copy a scenario's source directory into the run directory.
    pub(crate) fn materialize(&self, name: &str, source: &Path) -> Result<PathBuf, String> {
        let target = self.root.join("scenarios").join(name);
        std::fs::create_dir_all(&target)
            .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
        copy_tree(source, &target)?;
        Ok(target)
    }

    /// Tear the run directory down through the no-follow remover:
    /// everything when nothing is kept, or every subtree but the kept
    /// failures and their category scaffold. Leftovers are printed
    /// loudly, and a link is unlinked as a link, never followed.
    pub(crate) fn finish(&self, kept: &[PathBuf]) {
        if kept.is_empty() {
            let report = cleanrun::teardown_run(&self.root);
            if !report.clean() {
                cleanrun::emit_leftovers(&report);
            }
            return;
        }
        emit_scenario_leftovers(
            &self.bin_dir(),
            cleanrun::remove_scenario_dir(&self.bin_dir()),
        );
        prune(&self.root.join("scenarios"), kept);
        let _ = std::fs::remove_dir(&self.root);
    }
}

/// Remove the subtrees of `dir` that hold no kept failure: a kept
/// directory stays untouched, a directory above one keeps its scaffold,
/// and every other subtree goes through the no-follow remover.
fn prune(dir: &Path, kept: &[PathBuf]) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = read.flatten().collect();
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let path = item.path();
        let is_dir = item.file_type().is_ok_and(|t| t.is_dir());
        if !is_dir {
            eprintln!(
                "kdlith-fixtures: leftover: foreign file {} left in place (not deleted)",
                path.display()
            );
            continue;
        }
        if kept.iter().any(|k| k == &path) {
            continue;
        }
        if kept.iter().any(|k| k.starts_with(&path)) {
            prune(&path, kept);
            let _ = std::fs::remove_dir(&path);
        } else {
            emit_scenario_leftovers(&path, cleanrun::remove_scenario_dir(&path));
        }
    }
}

/// Print one subtree's unremovable leftovers loudly.
fn emit_scenario_leftovers(dir: &Path, leftovers: Vec<String>) {
    for path in leftovers {
        eprintln!(
            "kdlith-fixtures: leftover: could not remove {path} under {}",
            dir.display()
        );
    }
}

/// Remove one materialized scenario directory through the no-follow
/// remover; a planted symlink goes as a link, and any leftover is
/// recorded loudly instead of silently kept.
pub(crate) fn remove_scenario(dir: &Path) {
    emit_scenario_leftovers(dir, cleanrun::remove_scenario_dir(dir));
}

/// Recursive copy of regular files and directories. Symlinks and special
/// files are refused so a scenario cannot reach outside its own tree.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let read =
        std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    let mut items: Vec<_> = read
        .collect::<Result<_, _>>()
        .map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let src = item.path();
        let dst = to.join(item.file_name());
        let file_type = item
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", src.display()))?;
        if file_type.is_dir() {
            std::fs::create_dir(&dst)
                .map_err(|e| format!("cannot create {}: {e}", dst.display()))?;
            copy_tree(&src, &dst)?;
        } else if file_type.is_file() {
            std::fs::copy(&src, &dst)
                .map_err(|e| format!("cannot copy {} to {}: {e}", src.display(), dst.display()))?;
        } else {
            return Err(format!(
                "{} is neither a regular file nor a directory; scenarios may not contain links or special files",
                src.display()
            ));
        }
    }
    Ok(())
}

/// The run id: `<UTC yyyymmddThhmmssZ>-<pid>`.
fn run_id() -> String {
    format!("{}-{}", utc_stamp(SystemTime::now()), std::process::id())
}

/// Format an instant as `yyyymmddThhmmssZ` (civil-from-days conversion).
fn utc_stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z")
}
