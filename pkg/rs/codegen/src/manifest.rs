//! The output manifest and the ownership it proves: the writer that
//! refuses unowned overwrites (`KDLITH-GEN001`) and the drift check
//! that classifies missing, changed, and stale files
//! (`KDLITH-GEN002`). Generation never deletes anything.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};

use crate::emit::GeneratedTree;

/// The output manifest's file name, directly under the output root.
pub const MANIFEST_NAME: &str = "kdlith-manifest";

/// The diagnostic the writer refuses an unowned overwrite with.
pub(crate) const UNOWNED: &str = "KDLITH-GEN001";
/// The diagnostic the drift check classifies findings with.
pub(crate) const DRIFT: &str = "KDLITH-GEN002";
/// The provenance every generation diagnostic carries.
const ORIGIN: &str = "codegen";

/// Why a write or check could not proceed.
#[derive(Debug)]
#[non_exhaustive]
pub enum WriteError {
    /// One `KDLITH-GEN001` record per unowned path that exists; nothing
    /// was written.
    Refused(Vec<DiagnosticRecord>),
    /// The output manifest exists but does not parse: ownership cannot
    /// be established, so nothing is written.
    ManifestCorrupt { path: PathBuf, reason: String },
    /// A filesystem error.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(records) => {
                write!(f, "refusing to write: {} unowned path(s)", records.len())
            }
            Self::ManifestCorrupt { path, reason } => {
                write!(
                    f,
                    "the output manifest at {} is unreadable: {reason}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(f, "cannot use {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for WriteError {}

/// The manifest text for the owned paths, sorted, each listed once —
/// the manifest itself included by its caller.
pub(crate) fn manifest_text<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut owned: BTreeSet<&str> = paths.collect();
    owned.insert(MANIFEST_NAME);
    let mut text = String::new();
    text.push_str(crate::COMMENT_MARKER);
    text.push('\n');
    text.push_str(
        "# Generator-owned paths under this root, sorted; the writer refuses\n# to overwrite a path this list does not own.\n",
    );
    for path in owned {
        text.push_str(path);
        text.push('\n');
    }
    text
}

/// The paths an existing manifest owns, or the reason it cannot be
/// read. A manifest that is absent owns nothing.
fn owned_paths(out: &Path) -> Result<BTreeSet<String>, WriteError> {
    let manifest = out.join(MANIFEST_NAME);
    let text = match std::fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(source) => {
            return Err(WriteError::Io {
                path: manifest,
                source,
            });
        }
    };
    let mut owned = BTreeSet::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('/')
            || line
                .split('/')
                .any(|segment| segment.is_empty() || segment == "..")
            || !line
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
        {
            return Err(WriteError::ManifestCorrupt {
                path: manifest,
                reason: format!("the path entry `{line}` is not a relative owned path"),
            });
        }
        owned.insert(line.to_owned());
    }
    Ok(owned)
}

/// Write the tree under `out`, through the ownership guard: an
/// existing file the manifest does not own refuses the whole write
/// with one `KDLITH-GEN001` record per path. Nothing is deleted.
pub fn write(tree: &GeneratedTree, out: &Path) -> Result<(), WriteError> {
    let owned = owned_paths(out)?;
    let mut refusals = Vec::new();
    for path in tree.paths() {
        let target = out.join(path);
        if target.exists() && !owned.contains(path) {
            refusals.push(record(UNOWNED, &[path]));
        }
    }
    if !refusals.is_empty() {
        return Err(WriteError::Refused(refusals));
    }
    for (path, content) in &tree.files {
        let target = out.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| WriteError::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        std::fs::write(&target, content).map_err(|source| WriteError::Io {
            path: target,
            source,
        })?;
    }
    Ok(())
}

/// Compare the tree against `out` without writing: a generated path
/// that is absent is *missing*, one whose bytes differ is *changed*,
/// and a manifest-owned path the tree no longer generates is *stale*.
/// Each finding is one `KDLITH-GEN002` record; unowned files are
/// ignored.
pub fn check(tree: &GeneratedTree, out: &Path) -> Result<Vec<DiagnosticRecord>, WriteError> {
    let owned = owned_paths(out)?;
    let mut findings = Vec::new();
    let classified: BTreeSet<&str> = tree.paths().collect();
    for (path, content) in &tree.files {
        let target = out.join(path);
        match std::fs::read(&target) {
            Ok(bytes) if bytes != content.as_bytes() => {
                findings.push(record(DRIFT, &[path, "changed"]));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                findings.push(record(DRIFT, &[path, "missing"]));
            }
            Err(source) => {
                return Err(WriteError::Io {
                    path: target,
                    source,
                });
            }
        }
    }
    for path in &owned {
        if !classified.contains(path.as_str()) {
            findings.push(record(DRIFT, &[path, "stale"]));
        }
    }
    Ok(findings)
}

/// A registered code's record, message rendered from its template.
pub(crate) fn record(code: &'static str, values: &[&str]) -> DiagnosticRecord {
    let mut record = DiagnosticRecord::from_code(code)
        .unwrap_or_else(|| panic!("the generation code {code} is registered and never retired"));
    if let Some(definition) = lookup(code) {
        record = record.with_message(render(definition.message_template, values));
    }
    record.with_origin(ORIGIN)
}
