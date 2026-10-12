//! Suite discovery: a directory under the suite root that carries a
//! `scenario.json` manifest is one scenario, named by its `/`-joined path
//! relative to the root. Other directories (categories) are descended;
//! discovery is sorted so listings and trial order are stable.

use std::path::{Path, PathBuf};

use crate::scenario::MANIFEST;

/// A discovered scenario: its name and source directory.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
}

/// The result of scanning a suite root.
#[derive(Debug)]
pub(crate) struct Discovery {
    pub(crate) entries: Vec<Entry>,
    /// Manifest-less directories with no subdirectories (reported, not run).
    pub(crate) skipped: Vec<String>,
}

/// Scan the suite root recursively. A missing or unreadable directory, or
/// a name that is not UTF-8, is an error; an empty result is the caller's
/// to reject. Symlinked directories are not followed.
pub(crate) fn discover(root: &Path) -> Result<Discovery, String> {
    let mut found = Discovery {
        entries: Vec::new(),
        skipped: Vec::new(),
    };
    walk(root, "", &mut found)?;
    found.entries.sort_by(|a, b| a.name.cmp(&b.name));
    found.skipped.sort();
    Ok(found)
}

/// Visit the child directories of `dir`, whose suite-relative name is
/// `prefix` (empty for the root).
fn walk(dir: &Path, prefix: &str, found: &mut Discovery) -> Result<(), String> {
    for (name, path) in child_dirs(dir)? {
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if path.join(MANIFEST).exists() {
            found.entries.push(Entry {
                name: rel,
                dir: path,
            });
        } else if child_dirs(&path)?.is_empty() {
            found.skipped.push(rel);
        } else {
            walk(&path, &rel, found)?;
        }
    }
    Ok(())
}

/// The real (non-symlink) subdirectories of `dir` with their UTF-8 names.
fn child_dirs(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let read =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for item in read {
        let item = item.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let file_type = item
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", item.path().display()))?;
        if !file_type.is_dir() {
            continue;
        }
        let name = item.file_name().into_string().map_err(|raw| {
            format!(
                "suite directory name is not valid UTF-8: {raw:?} in {}",
                dir.display()
            )
        })?;
        out.push((name, item.path()));
    }
    Ok(out)
}
