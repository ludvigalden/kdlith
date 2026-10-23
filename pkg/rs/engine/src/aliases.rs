//! Host-registered import aliases: a named root — a directory or
//! documents held in memory — that `$name/<path>` imports address
//! wherever the importer lives, confined to that root rather than to
//! the entry document's directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::DiagnosticRecord;

use crate::definition::DiscoveryDecl;
use crate::diag::{IMPORT_GRAMMAR, IMPORT_UNRESOLVED, record};
use crate::names::Names;

const IMPORT_OUTSIDE_ALIAS: &str = "KDLITH-IMP007";
const ALIAS_INVALID: &str = "KDLITH-IMP008";

/// The root an import alias names: a directory on disk, or documents
/// held in memory keyed by their normalized path relative to the root
/// (`profiles/core.kdl`), such as documents embedded in a binary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AliasRoot {
    Fs(PathBuf),
    Embedded(BTreeMap<String, String>),
}

/// A registered alias as a reading resolves against it.
enum Root {
    /// The directory's canonical path.
    Fs(PathBuf),
    Embedded(BTreeMap<String, Rc<str>>),
}

/// Where a document an alias loaded sits: the alias and its normalized
/// path under the alias's root.
#[derive(Clone, Debug)]
pub(crate) struct AliasOrigin {
    pub(crate) alias: String,
    pub(crate) relative: String,
}

/// One document an alias import resolves to.
pub(crate) struct AliasTarget {
    /// The invocation's identity: the canonical file of a directory
    /// root, or the alias spelling of an embedded document.
    pub(crate) resolved: PathBuf,
    /// `$alias/<relative>`, wherever the root lives.
    pub(crate) shown: String,
    /// The text of an embedded document; a directory root's document is
    /// read from disk.
    pub(crate) text: Option<Rc<str>>,
    pub(crate) origin: AliasOrigin,
}

/// The aliases one reading admits, each validated before any document
/// is read.
#[derive(Default)]
pub(crate) struct Aliases {
    roots: BTreeMap<String, Root>,
}

impl Aliases {
    /// Validate the registered aliases: one `KDLITH-IMP008` per alias
    /// whose name is malformed, repeated, or also an anchor — supplied by
    /// the host or declared by a discovery grammar of the definition —
    /// or whose root does not resolve; every alias is checked, in
    /// registration order.
    pub(crate) fn admit(
        registered: &[(String, AliasRoot)],
        anchors: &[(String, PathBuf)],
        grammars: &[DiscoveryDecl],
        names: &Names,
        origin: &str,
    ) -> Result<Self, Vec<DiagnosticRecord>> {
        let mut roots = BTreeMap::new();
        let mut failures = Vec::new();
        let declared = |name: &str| {
            grammars
                .iter()
                .flat_map(|grammar| &grammar.anchors)
                .any(|anchor| names.resolve(anchor.name) == name)
        };
        for (name, root) in registered {
            let reason = if !is_kebab(name) {
                Some("the name is not a non-empty kebab name".to_owned())
            } else if roots.contains_key(name) {
                Some("the alias is registered more than once".to_owned())
            } else if anchors.iter().any(|(anchor, _)| anchor == name) || declared(name) {
                Some("an anchor of the same name is supplied or declared".to_owned())
            } else {
                match admit_root(root) {
                    Ok(root) => {
                        roots.insert(name.clone(), root);
                        None
                    }
                    Err(reason) => Some(reason),
                }
            };
            if let Some(reason) = reason {
                failures.push(record(ALIAS_INVALID, &[name, &reason], origin));
            }
        }
        if failures.is_empty() {
            Ok(Self { roots })
        } else {
            Err(failures)
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// The registered names, in name order.
    pub(crate) fn names(&self) -> Vec<&str> {
        self.roots.keys().map(String::as_str).collect()
    }

    /// The alias an import expression addresses: `$name/...` whose
    /// `name` is registered. Any other expression is not an alias
    /// import, whatever it spells.
    pub(crate) fn addressed<'e>(&self, expression: &'e str) -> Option<(&'e str, &'e str)> {
        let rest = expression.strip_prefix('$')?;
        let (name, path) = rest.split_once('/').unwrap_or((rest, ""));
        self.roots.contains_key(name).then_some((name, path))
    }

    /// Resolve `path`, relative to the root of `alias`, from the import
    /// `expression`: lexically normalized, refused when it leaves the
    /// root (`KDLITH-IMP007`), and admitted only as a literal path.
    pub(crate) fn resolve(
        &self,
        alias: &str,
        path: &str,
        expression: &str,
        origin: &str,
    ) -> Result<AliasTarget, Box<DiagnosticRecord>> {
        let escape = || Box::new(record(IMPORT_OUTSIDE_ALIAS, &[expression, alias], origin));
        let not_admitted =
            |reason: &str| Box::new(record(IMPORT_GRAMMAR, &[expression, reason], origin));
        if path.contains(['*', '{', '}', '$']) {
            return Err(not_admitted("an import alias addresses literal paths only"));
        }
        let relative = normalize(path).ok_or_else(escape)?;
        if relative.is_empty() {
            return Err(Box::new(record(
                IMPORT_UNRESOLVED,
                &[expression, "it names the alias root, not a document"],
                origin,
            )));
        }
        let shown = format!("${alias}/{relative}");
        let origin_of = AliasOrigin {
            alias: alias.to_owned(),
            relative: relative.clone(),
        };
        match self.roots.get(alias) {
            Some(Root::Fs(root)) => {
                let resolved = std::fs::canonicalize(root.join(&relative)).map_err(|error| {
                    Box::new(record(
                        IMPORT_UNRESOLVED,
                        &[&shown, &error.to_string()],
                        origin,
                    ))
                })?;
                // A link inside the root whose target lies outside it
                // leaves the root as surely as a `..` does.
                if !resolved.starts_with(root) {
                    return Err(escape());
                }
                if !resolved.is_file() {
                    return Err(Box::new(record(
                        IMPORT_UNRESOLVED,
                        &[&shown, "not a file"],
                        origin,
                    )));
                }
                Ok(AliasTarget {
                    resolved,
                    shown,
                    text: None,
                    origin: origin_of,
                })
            }
            Some(Root::Embedded(documents)) => match documents.get(&relative) {
                Some(text) => Ok(AliasTarget {
                    resolved: PathBuf::from(&shown),
                    shown,
                    text: Some(text.clone()),
                    origin: origin_of,
                }),
                None => Err(Box::new(record(
                    IMPORT_UNRESOLVED,
                    &[&shown, "the alias root holds no such document"],
                    origin,
                ))),
            },
            None => Err(Box::new(record(
                IMPORT_UNRESOLVED,
                &[expression, "no import alias of that name is registered"],
                origin,
            ))),
        }
    }

    /// Resolve a literal import a document the alias loaded makes:
    /// relative to that document's place under the root, never outside
    /// it.
    pub(crate) fn resolve_relative(
        &self,
        importer: &AliasOrigin,
        expression: &str,
        origin: &str,
    ) -> Result<AliasTarget, Box<DiagnosticRecord>> {
        if expression.starts_with('/') || Path::new(expression).is_absolute() {
            return Err(Box::new(record(
                IMPORT_OUTSIDE_ALIAS,
                &[expression, &importer.alias],
                origin,
            )));
        }
        let dir = importer
            .relative
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir);
        let joined = if dir.is_empty() {
            expression.to_owned()
        } else {
            format!("{dir}/{expression}")
        };
        self.resolve(&importer.alias, &joined, expression, origin)
    }

    /// Whether `alias` names a directory root, and so whether a document
    /// it loaded may expand grammar-admitted expressions on disk.
    pub(crate) fn fs_root(&self, alias: &str) -> Option<&Path> {
        match self.roots.get(alias) {
            Some(Root::Fs(root)) => Some(root),
            _ => None,
        }
    }

    /// Refuse a target a document of `alias` reached that lies outside
    /// the alias's root (`KDLITH-IMP007`).
    pub(crate) fn confine(
        &self,
        alias: &str,
        expression: &str,
        resolved: &Path,
        origin: &str,
    ) -> Result<(), Box<DiagnosticRecord>> {
        match self.fs_root(alias) {
            Some(root) if resolved.starts_with(root) => Ok(()),
            _ => Err(Box::new(record(
                IMPORT_OUTSIDE_ALIAS,
                &[expression, alias],
                origin,
            ))),
        }
    }
}

fn admit_root(root: &AliasRoot) -> Result<Root, String> {
    match root {
        AliasRoot::Fs(path) => {
            let canonical = std::fs::canonicalize(path)
                .map_err(|error| format!("root {} does not resolve: {error}", path.display()))?;
            if !canonical.is_dir() {
                return Err(format!("root {} is not a directory", path.display()));
            }
            Ok(Root::Fs(canonical))
        }
        AliasRoot::Embedded(documents) => {
            let mut held = BTreeMap::new();
            for (key, text) in documents {
                if normalize(key).as_deref() != Some(key.as_str()) || key.is_empty() {
                    return Err(format!(
                        "embedded document key {key:?} is not a normalized relative path"
                    ));
                }
                held.insert(key.clone(), Rc::from(text.as_str()));
            }
            Ok(Root::Embedded(held))
        }
    }
}

/// `path` with `.` segments dropped and each `..` taking back the
/// segment before it; `None` when a `..` would leave the root or the
/// path is absolute. Empty segments are dropped, so `a//b` is `a/b`.
fn normalize(path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
