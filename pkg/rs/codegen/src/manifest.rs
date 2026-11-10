//! The output manifest, the ownership it proves (`KDLITH-GEN001`), the
//! drift check (`KDLITH-GEN002`), and the sibling-packaging write guard
//! (`KDLITH-GEN008`). Generation never deletes anything.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};

use crate::Packaging;
use crate::emit::GeneratedTree;

/// The output manifest's file name, directly under the output root.
pub const MANIFEST_NAME: &str = "kdlith-manifest";

/// The diagnostic the writer refuses an unowned overwrite with.
pub(crate) const UNOWNED: &str = "KDLITH-GEN001";
/// The diagnostic the drift check classifies findings with.
pub(crate) const DRIFT: &str = "KDLITH-GEN002";
/// The diagnostic the writer refuses an unresolvable default
/// packaging with.
pub(crate) const UNRESOLVABLE: &str = "KDLITH-GEN008";
/// The provenance every generation diagnostic carries.
const ORIGIN: &str = "codegen";

/// Why a write or check could not proceed.
#[derive(Debug)]
#[non_exhaustive]
pub enum WriteError {
    /// One refusal record per reason (`KDLITH-GEN001` for an unowned
    /// path, `KDLITH-GEN008` for an unresolvable sibling dependency);
    /// nothing was written.
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

/// The prefix of a provenance line: a comment to every manifest
/// reader, so a reader that predates provenance skips it, and a key
/// line to this one.
const PROVENANCE: &str = "#@ ";

/// The provenance line naming the generator, up to its version value.
const GENERATOR_LINE: &str = "#@ generator kdlith version=";

/// What a generated output records about how it was generated: the
/// generator version and the invocation, never a path or a time, so two
/// runs over the same inputs write the same bytes.
pub(crate) struct Provenance<'a> {
    pub(crate) form: &'a str,
    pub(crate) crate_name: &'a str,
    pub(crate) definition: &'a str,
    pub(crate) definition_version: &'a str,
    pub(crate) packaging: &'a Packaging,
}

/// The manifest text: the marker, the header comments, the provenance
/// lines, then the owned paths, sorted, each listed once — the manifest
/// itself included by its caller.
pub(crate) fn manifest_text<'a>(
    provenance: &Provenance<'_>,
    paths: impl Iterator<Item = &'a str>,
) -> String {
    let mut owned: BTreeSet<&str> = paths.collect();
    owned.insert(MANIFEST_NAME);
    let mut text = String::new();
    text.push_str(crate::COMMENT_MARKER);
    text.push('\n');
    text.push_str(
        "# Generator-owned paths under this root, sorted; the writer refuses\n# to overwrite a path this list does not own.\n",
    );
    let packaging = match provenance.packaging {
        Packaging::Sibling => "sibling",
        Packaging::Path(_) => "path",
        Packaging::Registry(_) => "registry",
        Packaging::Workspace => "workspace",
    };
    for line in [
        format!("{GENERATOR_LINE}{}", quoted(crate::GENERATOR_VERSION)),
        format!("{PROVENANCE}form {}", provenance.form),
        format!("{PROVENANCE}crate {}", provenance.crate_name),
        format!(
            "{PROVENANCE}definition {} version={}",
            quoted(provenance.definition),
            quoted(provenance.definition_version)
        ),
        format!("{PROVENANCE}packaging {packaging}"),
    ] {
        text.push_str(&line);
        text.push('\n');
    }
    for path in owned {
        text.push_str(path);
        text.push('\n');
    }
    text
}

/// A KDL string: quoted, with its quotes and backslashes escaped.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The generator version a manifest's provenance records, if it records
/// one: the value of its first generator line.
fn recorded_version(manifest: &str) -> Option<String> {
    let value = manifest
        .lines()
        .find_map(|line| line.strip_prefix(GENERATOR_LINE))?;
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// `text` with its first line starting `prefix` replaced by `line`;
/// `text` unchanged when no line starts so.
fn replace_line(text: &str, prefix: &str, line: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut replaced = false;
    for piece in text.split_inclusive('\n') {
        if !replaced && piece.starts_with(prefix) {
            out.push_str(line);
            if piece.ends_with('\n') {
                out.push('\n');
            }
            replaced = true;
        } else {
            out.push_str(piece);
        }
    }
    out
}

/// An existing file's bytes as the drift check compares them: the
/// generator version reads as the running generator's, in the manifest
/// and, while it equals the recorded version, in `Cargo.toml`.
fn version_blind(path: &str, existing: &[u8], recorded: Option<&str>) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(existing) else {
        return existing.to_vec();
    };
    let current = format!("{GENERATOR_LINE}{}", quoted(crate::GENERATOR_VERSION));
    if path == MANIFEST_NAME {
        return replace_line(text, GENERATOR_LINE, &current).into_bytes();
    }
    if path == "Cargo.toml"
        && let Some(recorded) = recorded
    {
        let stale = format!("version = {}", quoted(recorded));
        let fresh = format!("version = {}", quoted(crate::GENERATOR_VERSION));
        return package_version(text, &stale, &fresh).into_bytes();
    }
    existing.to_vec()
}

/// `text` with the `[package]` table's `stale` version line replaced by
/// `fresh`; a version line anywhere else is left alone.
fn package_version(text: &str, stale: &str, fresh: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_package = false;
    let mut replaced = false;
    for piece in text.split_inclusive('\n') {
        let line = piece.trim_end_matches('\n');
        if line.starts_with('[') {
            in_package = line == "[package]";
        }
        if in_package && !replaced && line == stale {
            out.push_str(fresh);
            if piece.ends_with('\n') {
                out.push('\n');
            }
            replaced = true;
        } else {
            out.push_str(piece);
        }
    }
    out
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
/// with one `KDLITH-GEN001` record per path. Under the default
/// sibling packaging, every kdlith dependency directory the manifest
/// names must hold a crate beside `out`, or the write refuses with one
/// `KDLITH-GEN008` record per unresolvable dependency — a manifest of
/// broken path dependencies is never written. Nothing is deleted.
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
    if tree.packaging == Packaging::Sibling {
        // The manifest writes `../<dir>` path dependencies, resolved
        // lexically against the output crate's parent so an `out` that
        // does not exist yet still checks the layout it will land in.
        let sibling_root = out.parent().unwrap_or(Path::new(""));
        for dir in crate::dependency_dirs(tree.kind) {
            let crate_dir = sibling_root.join(dir);
            if !crate_dir.join("Cargo.toml").is_file() {
                let name = format!("kdlith-{dir}");
                let shown = crate_dir.display().to_string();
                refusals.push(record(UNRESOLVABLE, &[&name, &shown]));
            }
        }
        if !refusals.is_empty() {
            return Err(WriteError::Refused(refusals));
        }
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
/// The generator version alone is never a difference: the manifest's
/// recorded version, and a `Cargo.toml` `[package]` version equal to
/// it, compare as the running generator's.
/// Each finding is one `KDLITH-GEN002` record; unowned files are
/// ignored.
pub fn check(tree: &GeneratedTree, out: &Path) -> Result<Vec<DiagnosticRecord>, WriteError> {
    let owned = owned_paths(out)?;
    let recorded = std::fs::read_to_string(out.join(MANIFEST_NAME))
        .ok()
        .and_then(|manifest| recorded_version(&manifest));
    let mut findings = Vec::new();
    let classified: BTreeSet<&str> = tree.paths().collect();
    for (path, content) in &tree.files {
        let target = out.join(path);
        match std::fs::read(&target) {
            Ok(bytes) if version_blind(path, &bytes, recorded.as_deref()) != content.as_bytes() => {
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
