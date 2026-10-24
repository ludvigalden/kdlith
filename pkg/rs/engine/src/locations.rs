//! Locations and sources: every import path names a document in a
//! named tree of files and never leaves the tree it resolved against;
//! hosts add trees backed by a directory or by embedded files.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::Value;

use crate::definition::{LocationsDecl, Patterns};
use crate::diag::{
    ASSET_MISMATCH, LOCATION_NOT_ADMITTED, LOCATION_OUTSIDE_SOURCE, LOCATION_UNRESOLVED,
    SOURCE_UNKNOWN, SOURCE_UNREADABLE, Site, record,
};
use crate::digest::{bytes_digest, files_digest, tree_digest};
use crate::sources::{Entry, Tree, View};
use crate::syntax::{ParsedDocument, SyntaxNode};
use crate::types::{AssetBase, AssetDecl, AssetKind};

/// Why a location naming no file in its source does not resolve.
const NO_SUCH_FILE: &str = "the source holds no such file";
const NOT_A_FILE: &str = "not a file";

/// The tree a named source holds: a directory on disk, or files held in
/// memory keyed by their normalized path relative to the source's root
/// (`profiles/core.kdl`), such as files embedded in a host binary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceTree {
    Dir(PathBuf),
    Embedded(BTreeMap<String, String>),
}

impl SourceTree {
    /// A source backed by the directory `root`.
    pub fn dir(root: impl Into<PathBuf>) -> Self {
        SourceTree::Dir(root.into())
    }

    /// A source backed by `files`, each keyed by its normalized path
    /// relative to the source's root.
    pub fn embedded<I, K, V>(files: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        SourceTree::Embedded(
            files
                .into_iter()
                .map(|(key, text)| (key.into(), text.into()))
                .collect(),
        )
    }
}

/// The root of the local source one entry document belongs to: the
/// project root, or, for an entry outside it, the entry's own directory.
#[derive(Debug)]
pub(crate) struct LocalRoot {
    canonical: PathBuf,
    /// How a path under the root is shown: empty for the project root,
    /// so its documents show root-relative, else the entry's directory
    /// as the invocation spelled it.
    shown: PathBuf,
}

impl LocalRoot {
    /// The project root at its canonical path.
    pub(crate) fn project(canonical: PathBuf) -> Self {
        Self {
            canonical,
            shown: PathBuf::new(),
        }
    }
}

/// The source a document was loaded from.
#[derive(Clone, Debug)]
pub(crate) enum Home {
    Local(Rc<LocalRoot>),
    /// A dialect definition document read from the local source: its
    /// relative locations follow its own directory unconfined.
    Unconfined,
    Named(String),
}

/// Where a document sits: its source and its normalized path under that
/// source's root. A document held in memory as an entry has none.
#[derive(Clone, Debug)]
pub(crate) struct Place {
    pub(crate) home: Home,
    pub(crate) relative: String,
}

/// One document a location resolved to.
pub(crate) struct Target {
    /// The invocation's identity: the canonical file of a directory-backed
    /// source, or the shown spelling of an embedded file.
    pub(crate) resolved: PathBuf,
    pub(crate) shown: String,
    /// The text of an embedded file; a directory-backed file is read
    /// from disk.
    pub(crate) text: Option<Rc<str>>,
    pub(crate) place: Place,
}

/// The source set a reading resolved its assets against, kept so a host
/// opens each asset through the same trees and confinement.
#[derive(Clone)]
pub(crate) struct AssetSet(pub(crate) Rc<Sources>);

impl std::fmt::Debug for AssetSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetSet({:016x})", self.0.fingerprint)
    }
}

/// One asset a location resolved to, shown as its source shows it.
pub(crate) struct AssetTarget {
    pub(crate) shown: String,
    /// The source's name; empty for the local source.
    pub(crate) source: String,
    pub(crate) digest: Option<String>,
}

/// Why an asset location names nothing usable.
enum Missing {
    NoSuchFile,
    Outside,
    Unreadable(String),
}

/// What an asset location names; its bytes are read only when a digest
/// or a host's open asks for them.
enum AssetContent {
    File(FileContent),
    Dir(DirContent),
}

enum FileContent {
    /// The file's canonical path.
    Disk(PathBuf),
    Held(Rc<str>),
}

enum DirContent {
    /// The directory's canonical path.
    Disk(PathBuf),
    /// The embedded files under it, keyed relative to it.
    Held(Vec<(String, Rc<str>)>),
}

impl FileContent {
    fn bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            FileContent::Disk(path) => std::fs::read(path).map_err(|error| error.to_string()),
            FileContent::Held(text) => Ok(text.as_bytes().to_vec()),
        }
    }
}

impl AssetContent {
    /// The content digest: a file's bytes, or a directory's canonical
    /// tree digest, as a source is pinned by.
    fn digest(&self) -> Result<String, String> {
        match self {
            AssetContent::File(file) => Ok(bytes_digest(&file.bytes()?)),
            AssetContent::Dir(DirContent::Held(files)) => files_digest(
                files
                    .iter()
                    .map(|(key, text)| (key.as_str(), text.as_bytes())),
            ),
            AssetContent::Dir(DirContent::Disk(root)) => tree_digest(root),
        }
    }
}

/// An embedded source's file, or the directory its keys imply.
fn embedded_asset(
    files: &BTreeMap<String, Rc<str>>,
    relative: &str,
) -> Result<AssetContent, Missing> {
    if let Some(text) = files.get(relative) {
        return Ok(AssetContent::File(FileContent::Held(text.clone())));
    }
    let prefix = format!("{relative}/");
    let under: Vec<(String, Rc<str>)> = files
        .iter()
        .filter_map(|(key, text)| Some((key.strip_prefix(&prefix)?.to_owned(), text.clone())))
        .collect();
    if under.is_empty() {
        return Err(Missing::NoSuchFile);
    }
    Ok(AssetContent::Dir(DirContent::Held(under)))
}

/// A file or directory under the canonical root of a directory-backed
/// source; a link whose target leaves the root leaves the source.
fn disk_asset(root: &Path, relative: &str) -> Result<AssetContent, Missing> {
    let resolved =
        std::fs::canonicalize(root.join(relative)).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => Missing::NoSuchFile,
            _ => Missing::Unreadable(error.to_string()),
        })?;
    if !resolved.starts_with(root) {
        return Err(Missing::Outside);
    }
    if resolved.is_dir() {
        Ok(AssetContent::Dir(DirContent::Disk(resolved)))
    } else if resolved.is_file() {
        Ok(AssetContent::File(FileContent::Disk(resolved)))
    } else {
        Err(Missing::Unreadable(
            "neither a file nor a directory".to_owned(),
        ))
    }
}

/// Whether a file name ends in `.EXTENSION` after a non-empty stem.
fn has_extension(name: &str, extension: &str) -> bool {
    name.strip_suffix(extension)
        .and_then(|stem| stem.strip_suffix('.'))
        .is_some_and(|stem| !stem.is_empty())
}

/// The sources one reading resolves locations against, each validated
/// before any document is read (see `crate::sources`).
pub(crate) struct Sources {
    /// The project root, canonical: the local source's root.
    pub(crate) root: Rc<LocalRoot>,
    pub(crate) named: BTreeMap<String, Entry>,
    /// Identifies the trees: the root's and each directory's canonical
    /// path, each embedded file's key and text, each origin's store path.
    pub(crate) fingerprint: u64,
    /// The named sources a location of the reading entered.
    pub(crate) used: RefCell<BTreeSet<String>>,
}

impl Sources {
    /// The fingerprint of the trees this source set resolves against.
    pub(crate) fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// The place of an entry document read from disk: under the project
    /// root, the local source; otherwise an ad hoc local source rooted
    /// at its own directory. A dialect definition's entries resolve
    /// their relative locations unconfined.
    pub(crate) fn entry_place(&self, canonical: &Path, shown: &str, confined: bool) -> Place {
        let file = canonical.file_name().map_or_else(String::new, portable);
        if !confined {
            return Place {
                home: Home::Unconfined,
                relative: file,
            };
        }
        if let Ok(relative) = canonical.strip_prefix(&self.root.canonical) {
            return Place {
                home: Home::Local(self.root.clone()),
                relative: portable(relative.as_os_str()),
            };
        }
        let dir = canonical.parent().unwrap_or(Path::new("")).to_owned();
        let shown = Path::new(shown)
            .parent()
            .unwrap_or(Path::new(""))
            .to_owned();
        Place {
            home: Home::Local(Rc::new(LocalRoot {
                canonical: dir,
                shown,
            })),
            relative: file,
        }
    }

    /// Resolve the location `expression`, written in the document
    /// `importer` names (`None` for a document held in memory), under the
    /// locations rule `rule`.
    pub(crate) fn resolve(
        &self,
        expression: &str,
        importer: Option<Importer<'_>>,
        rule: Option<&LocationsDecl>,
        origin: &str,
    ) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
        let parsed = parse(expression, rule).map_err(|error| error.record(expression, origin))?;
        let (home, start, rest) = match &parsed.base {
            Base::Named(name) => (
                self.named_home(expression, name, origin)?,
                String::new(),
                parsed.path.as_str(),
            ),
            Base::Root | Base::Relative => {
                let Some(importer) = importer else {
                    return Err(unresolved(
                        expression,
                        "a document held in memory belongs to no source; name one with $name/",
                        origin,
                    ));
                };
                let place = importer.place;
                if let Home::Unconfined = place.home {
                    return self.unconfined(expression, &parsed, importer, rule, origin);
                }
                let start = match parsed.base {
                    Base::Root => String::new(),
                    _ => parent(&place.relative).to_owned(),
                };
                (place.home.clone(), start, parsed.path.as_str())
            }
        };
        let outside = || {
            Box::new(record(
                LOCATION_OUTSIDE_SOURCE,
                &[expression, &home_name(&home)],
                origin,
            ))
        };
        if parsed.patterns {
            let pattern = join_pattern(&start, rest).ok_or_else(outside)?;
            return self.expand(expression, &home, &pattern, rule, origin);
        }
        let relative = normalize(&join(&start, rest)).ok_or_else(outside)?;
        if relative.is_empty() {
            return Err(unresolved(
                expression,
                "it names the source's root, not a document",
                origin,
            ));
        }
        self.locate(expression, &home, relative, origin)
            .map(|target| vec![target])
    }

    /// The home of `$name/`: an unregistered name is `KDLITH-LOC003`,
    /// and an origin the store does not hold, or holds under another
    /// digest than its pin, fails as it is entered.
    fn named_home(
        &self,
        expression: &str,
        name: &str,
        origin: &str,
    ) -> Result<Home, Box<DiagnosticRecord>> {
        if self.holds(name) {
            self.enter(name, origin)?;
            return Ok(Home::Named(name.to_owned()));
        }
        let known: Vec<&str> = self.named.keys().map(String::as_str).collect();
        let reason = if known.is_empty() {
            "the reading has no named sources".to_owned()
        } else {
            format!("the reading's sources are {}", known.join(", "))
        };
        Err(Box::new(record(
            SOURCE_UNKNOWN,
            &[expression, name, &reason],
            origin,
        )))
    }

    /// The one document `relative` names under `home`'s root.
    fn locate(
        &self,
        expression: &str,
        home: &Home,
        relative: String,
        origin: &str,
    ) -> Result<Target, Box<DiagnosticRecord>> {
        let shown = shown(home, &relative);
        let outside = || {
            Box::new(record(
                LOCATION_OUTSIDE_SOURCE,
                &[expression, &home_name(home)],
                origin,
            ))
        };
        let root = match home {
            Home::Local(root) => root.canonical.clone(),
            Home::Named(name) => match self.view(name) {
                Some(View::Dir(root)) => root.to_path_buf(),
                Some(View::Embedded(files)) => {
                    return match files.get(&relative) {
                        Some(text) => Ok(Target {
                            resolved: PathBuf::from(&shown),
                            shown,
                            text: Some(text.clone()),
                            place: Place {
                                home: home.clone(),
                                relative,
                            },
                        }),
                        None => {
                            let prefix = format!("{relative}/");
                            let dir = files.keys().any(|key| key.starts_with(&prefix));
                            let reason = if dir { NOT_A_FILE } else { NO_SUCH_FILE };
                            Err(unresolved(&shown, reason, origin))
                        }
                    };
                }
                None => {
                    return Err(unresolved(
                        expression,
                        "the source is not registered",
                        origin,
                    ));
                }
            },
            Home::Unconfined => return Err(outside()),
        };
        // A missing file reads alike whatever backs the source.
        let resolved = std::fs::canonicalize(root.join(&relative)).map_err(|error| {
            let reason = match error.kind() {
                std::io::ErrorKind::NotFound => NO_SUCH_FILE.to_owned(),
                _ => error.to_string(),
            };
            unresolved(&shown, &reason, origin)
        })?;
        // A link inside the source whose target lies outside it leaves
        // the source as surely as a `..` does.
        if !resolved.starts_with(&root) {
            return Err(outside());
        }
        if !resolved.is_file() {
            return Err(unresolved(&shown, NOT_A_FILE, origin));
        }
        Ok(Target {
            resolved,
            shown,
            text: None,
            place: Place {
                home: home.clone(),
                relative,
            },
        })
    }

    /// The documents `pattern`, normalized under `home`'s root, matches:
    /// a walk of a directory-backed source or the keys of an embedded
    /// one, in portable-path-byte order. Zero matches do not resolve
    /// unless the rule allows an empty match.
    fn expand(
        &self,
        expression: &str,
        home: &Home,
        pattern: &[Segment],
        rule: Option<&LocationsDecl>,
        origin: &str,
    ) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
        let candidates = self.candidates(home, pattern, origin)?;
        if candidates.is_empty() && !rule.is_some_and(|rule| rule.allow_empty) {
            return Err(unresolved(
                expression,
                "the pattern matches no document",
                origin,
            ));
        }
        candidates
            .into_iter()
            .map(|relative| self.locate(expression, home, relative, origin))
            .collect()
    }

    /// The files under `home`'s root `pattern` matches, in
    /// portable-path-byte order.
    fn candidates(
        &self,
        home: &Home,
        pattern: &[Segment],
        origin: &str,
    ) -> Result<Vec<String>, Box<DiagnosticRecord>> {
        Ok(match home {
            Home::Local(root) => walk_matches(&root.canonical, pattern, origin)?,
            Home::Named(name) => match self.view(name) {
                Some(View::Dir(root)) => walk_matches(root, pattern, origin)?,
                Some(View::Embedded(files)) => files
                    .keys()
                    .filter(|key| matches(pattern, &key.split('/').collect::<Vec<_>>()))
                    .cloned()
                    .collect(),
                None => Vec::new(),
            },
            Home::Unconfined => Vec::new(),
        })
    }

    /// Resolve the asset location `expression`, written in the document
    /// at `place` (`None` for a document held in memory), as the asset
    /// type `ty` declares: located as an import location is, except that
    /// a relative spelling starts at the source's root under `base
    /// source`, and a pattern is admitted only where the type enables it
    /// and the value is one item of a list. Each asset is checked for
    /// existence, kind, and extension, and read only for its digest.
    pub(crate) fn resolve_asset(
        &self,
        expression: &str,
        place: Option<&Place>,
        (ty, decl): (&str, &AssetDecl),
        listed: bool,
        origin: &str,
    ) -> Result<Vec<AssetTarget>, Box<DiagnosticRecord>> {
        let rule = decl.patterns.then(|| LocationsDecl {
            patterns: Patterns {
                star: true,
                doublestar: true,
                braces: true,
            },
            allow_empty: false,
            site: Site::new(0, ByteSpan::new(0, 0)),
        });
        let parsed =
            parse(expression, rule.as_ref()).map_err(|error| error.record(expression, origin))?;
        if parsed.patterns && !listed {
            let reason = NotAdmitted("a pattern expands only as one item of a list".to_owned());
            return Err(reason.record(expression, origin));
        }
        let (home, start) = match &parsed.base {
            Base::Named(name) => (self.named_home(expression, name, origin)?, String::new()),
            Base::Root | Base::Relative => {
                let Some(place) = place else {
                    return Err(unresolved(
                        expression,
                        "a document held in memory belongs to no source; name one with $name/",
                        origin,
                    ));
                };
                let start = match (&parsed.base, decl.base) {
                    (Base::Relative, AssetBase::Document) => parent(&place.relative).to_owned(),
                    _ => String::new(),
                };
                (place.home.clone(), start)
            }
        };
        let outside = || {
            Box::new(record(
                LOCATION_OUTSIDE_SOURCE,
                &[expression, &home_name(&home)],
                origin,
            ))
        };
        if let Home::Unconfined = home {
            return Err(outside());
        }
        let relatives = if parsed.patterns {
            let pattern = join_pattern(&start, &parsed.path).ok_or_else(outside)?;
            let found = self.candidates(&home, &pattern, origin)?;
            if found.is_empty() {
                return Err(unresolved(
                    expression,
                    "the pattern matches no file",
                    origin,
                ));
            }
            found
        } else {
            let relative = normalize(&join(&start, &parsed.path)).ok_or_else(outside)?;
            if relative.is_empty() {
                return Err(unresolved(
                    expression,
                    "it names the source's root, not an asset",
                    origin,
                ));
            }
            vec![relative]
        };
        relatives
            .into_iter()
            .map(|relative| self.asset(expression, &home, relative, (ty, decl), origin))
            .collect()
    }

    /// The asset `relative` names under `home`'s root, checked against
    /// its type.
    fn asset(
        &self,
        expression: &str,
        home: &Home,
        relative: String,
        (ty, decl): (&str, &AssetDecl),
        origin: &str,
    ) -> Result<AssetTarget, Box<DiagnosticRecord>> {
        let shown = shown(home, &relative);
        let content = self
            .content(home, &relative)
            .map_err(|missing| match missing {
                Missing::NoSuchFile => unresolved(&shown, NO_SUCH_FILE, origin),
                Missing::Unreadable(reason) => unresolved(&shown, &reason, origin),
                Missing::Outside => Box::new(record(
                    LOCATION_OUTSIDE_SOURCE,
                    &[expression, &home_name(home)],
                    origin,
                )),
            })?;
        let mismatch =
            |reason: &str| Box::new(record(ASSET_MISMATCH, &[&shown, ty, reason], origin));
        match (decl.kind, &content) {
            (AssetKind::File, AssetContent::Dir(_)) => {
                return Err(mismatch("it is a directory; the type admits a file"));
            }
            (AssetKind::Dir, AssetContent::File(_)) => {
                return Err(mismatch("it is a file; the type admits a directory"));
            }
            _ => {}
        }
        let name = relative.rsplit('/').next().unwrap_or_default();
        if !decl.extensions.is_empty()
            && !decl
                .extensions
                .iter()
                .any(|extension| has_extension(name, extension))
        {
            let admitted: Vec<String> = decl.extensions.iter().map(|e| format!(".{e}")).collect();
            let reason = format!("its name does not end in {}", admitted.join(" or "));
            return Err(mismatch(&reason));
        }
        let digest = if decl.digest {
            let digest = content
                .digest()
                .map_err(|reason| unresolved(&shown, &reason, origin))?;
            Some(digest)
        } else {
            None
        };
        let source = match home {
            Home::Named(name) => name.clone(),
            _ => String::new(),
        };
        Ok(AssetTarget {
            shown,
            source,
            digest,
        })
    }

    /// What `relative` names under `home`'s root.
    fn content(&self, home: &Home, relative: &str) -> Result<AssetContent, Missing> {
        match home {
            Home::Named(name) => match self.view(name) {
                Some(View::Embedded(files)) => embedded_asset(files, relative),
                Some(View::Dir(root)) => disk_asset(root, relative),
                None => Err(Missing::NoSuchFile),
            },
            Home::Local(root) => disk_asset(&root.canonical, relative),
            Home::Unconfined => Err(Missing::Outside),
        }
    }

    /// The bytes of the file asset a reading resolved, located again
    /// from its source and shown location under this source set, so a
    /// host reading it stays inside the same confinement.
    pub(crate) fn open_asset(&self, source: &str, location: &str) -> Result<Vec<u8>, String> {
        let (home, relative) = if source.is_empty() {
            (Home::Local(self.root.clone()), location)
        } else {
            if !self.holds(source) {
                return Err(format!("the reading has no source ${source}"));
            }
            self.enter(source, "data-document")
                .map_err(|failure| failure.message.unwrap_or_default())?;
            let prefix = format!("${source}/");
            let relative = location
                .strip_prefix(&prefix)
                .ok_or_else(|| format!("{location} is not a location in ${source}"))?;
            (Home::Named(source.to_owned()), relative)
        };
        if relative.is_empty() || normalize(relative).as_deref() != Some(relative) {
            return Err(format!("{location} is not a normalized location"));
        }
        match self.content(&home, relative) {
            Ok(AssetContent::File(file)) => file.bytes(),
            Ok(AssetContent::Dir(_)) => Err(format!("{location} is a directory, not a file")),
            Err(Missing::NoSuchFile) => Err(format!("{location}: {NO_SUCH_FILE}")),
            Err(Missing::Outside) => Err(format!("{location} resolves outside its source")),
            Err(Missing::Unreadable(reason)) => Err(format!("{location}: {reason}")),
        }
    }

    /// A relative or root location written in an unconfined definition
    /// document: resolved from its own directory, or from the project
    /// root, and shown relative to the importer as shown, or to the root.
    fn unconfined(
        &self,
        expression: &str,
        parsed: &Parsed,
        importer: Importer<'_>,
        rule: Option<&LocationsDecl>,
        origin: &str,
    ) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
        let (dir, shown_dir) = match parsed.base {
            Base::Root => (self.root.canonical.clone(), PathBuf::new()),
            _ => (
                importer.file.parent().unwrap_or(Path::new("")).to_owned(),
                Path::new(importer.shown)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_owned(),
            ),
        };
        let relatives: Vec<String> = if parsed.patterns {
            let pattern = parse_segments(&parsed.path);
            let found = walk_matches(&dir, &pattern, origin)?;
            if found.is_empty() && !rule.is_some_and(|rule| rule.allow_empty) {
                return Err(unresolved(
                    expression,
                    "the pattern matches no document",
                    origin,
                ));
            }
            found
        } else {
            vec![parsed.path.clone()]
        };
        let mut targets = Vec::with_capacity(relatives.len());
        for relative in relatives {
            let shown = shown_dir.join(&relative).display().to_string();
            let resolved = std::fs::canonicalize(dir.join(&relative))
                .map_err(|error| unresolved(&shown, &error.to_string(), origin))?;
            if !resolved.is_file() {
                return Err(unresolved(&shown, "not a file", origin));
            }
            let file = resolved.file_name().map_or_else(String::new, portable);
            targets.push(Target {
                resolved,
                shown,
                text: None,
                place: Place {
                    home: Home::Unconfined,
                    relative: file,
                },
            });
        }
        Ok(targets)
    }
}

/// The document a location is written in.
#[derive(Clone, Copy)]
pub(crate) struct Importer<'i> {
    pub(crate) place: &'i Place,
    /// Its canonical file.
    pub(crate) file: &'i Path,
    /// Its path as the reading shows it.
    pub(crate) shown: &'i str,
}

/// How a location starts.
enum Base {
    /// Relative to the importing document's directory.
    Relative,
    /// From the importing document's source root (a leading `/`).
    Root,
    /// From the root of the named source (`$name/`).
    Named(String),
}

/// A location parsed and checked against its rule.
struct Parsed {
    base: Base,
    /// The path after its base, as written.
    path: String,
    /// Whether the path uses pattern syntax the rule admits.
    patterns: bool,
}

/// Why a location's syntax is not admitted (`KDLITH-LOC004`).
#[derive(Debug)]
pub(crate) struct NotAdmitted(String);

impl NotAdmitted {
    fn record(&self, expression: &str, origin: &str) -> Box<DiagnosticRecord> {
        Box::new(record(
            LOCATION_NOT_ADMITTED,
            &[expression, &self.0],
            origin,
        ))
    }

    /// The `KDLITH-LOC004` record of an import written at `site`.
    pub(crate) fn values<'a>(&'a self, expression: &'a str) -> [&'a str; 2] {
        [expression, &self.0]
    }
}

/// Check a location's syntax under `rule` without resolving it: the
/// compile-time check of a schema's own imports.
pub(crate) fn check(expression: &str, rule: Option<&LocationsDecl>) -> Result<(), NotAdmitted> {
    parse(expression, rule).map(|_| ())
}

fn parse(expression: &str, rule: Option<&LocationsDecl>) -> Result<Parsed, NotAdmitted> {
    let not_admitted = |reason: &str| NotAdmitted(reason.to_owned());
    if expression.is_empty() {
        return Err(not_admitted("it is empty"));
    }
    if expression.contains('\\') {
        return Err(not_admitted(
            "it uses a backslash; locations separate with /",
        ));
    }
    let (base, path) = if let Some(rest) = expression.strip_prefix('$') {
        let (name, path) = rest.split_once('/').unwrap_or((rest, ""));
        if name.starts_with('{') || !is_source_name(name) {
            return Err(not_admitted(
                "it uses environment-variable interpolation or a malformed $name",
            ));
        }
        if path.split('/').all(|segment| segment.is_empty()) {
            return Err(not_admitted(
                "it names a source without a path; write $name/PATH",
            ));
        }
        (Base::Named(name.to_owned()), path)
    } else if let Some(rest) = expression.strip_prefix('/') {
        (Base::Root, rest)
    } else {
        (Base::Relative, expression)
    };
    if path.contains('$') {
        return Err(not_admitted(
            "it uses environment-variable interpolation or a malformed $name",
        ));
    }
    if path.contains(['?', '[']) {
        return Err(not_admitted(
            "it uses pattern syntax no rule defines (only star, doublestar, and braces)",
        ));
    }
    let admitted = rule.map_or_else(Patterns::default, |rule| rule.patterns);
    let mut patterns = false;
    let mut after_pattern = false;
    for segment in path.split('/') {
        let doublestar = segment.contains("**");
        let star = !doublestar && segment.contains('*');
        let braces = segment.contains(['{', '}']);
        if doublestar && !admitted.doublestar {
            return Err(not_admitted(
                "it uses doublestar, which no rule here admits",
            ));
        }
        if star && !admitted.star {
            return Err(not_admitted("it uses star, which no rule here admits"));
        }
        if braces {
            if !admitted.braces {
                return Err(not_admitted(
                    "it uses brace alternatives, which no rule here admits",
                ));
            }
            if !balanced_alternatives(segment) {
                return Err(not_admitted("it has malformed brace alternatives"));
            }
        }
        if after_pattern && matches!(segment, "." | "..") {
            return Err(not_admitted("it walks back up after a pattern segment"));
        }
        if doublestar || star || braces {
            patterns = true;
            after_pattern = true;
        }
    }
    Ok(Parsed {
        base,
        path: path.to_owned(),
        patterns,
    })
}

/// One segment of a pattern location.
#[derive(Clone, Debug)]
enum Segment {
    Literal(String),
    /// `*` and `{a,b}` within one path segment.
    Pattern(String),
    /// `**`: zero or more directory segments.
    Doublestar,
}

fn parse_segments(path: &str) -> Vec<Segment> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(|segment| {
            if segment.contains("**") {
                Segment::Doublestar
            } else if segment.contains(['*', '{', '}']) {
                Segment::Pattern(segment.to_owned())
            } else {
                Segment::Literal(segment.to_owned())
            }
        })
        .collect()
}

/// A pattern path joined onto `start` with its literal prefix normalized
/// lexically; `None` when the prefix leaves the root.
fn join_pattern(start: &str, rest: &str) -> Option<Vec<Segment>> {
    let joined = join(start, rest);
    let segments: Vec<&str> = joined.split('/').collect();
    let first = segments
        .iter()
        .position(|segment| segment.contains(['*', '{', '}']))
        .unwrap_or(segments.len());
    let prefix = normalize(&segments[..first].join("/"))?;
    let mut pattern = parse_segments(&prefix);
    pattern.extend(parse_segments(&segments[first..].join("/")));
    Some(pattern)
}

/// Every file under `root` whose root-relative path `pattern` matches,
/// in portable-path-byte order. Linked directories are not walked; a
/// directory that cannot be read fails closed (`KDLITH-SRC001`).
fn walk_matches(
    root: &Path,
    pattern: &[Segment],
    origin: &str,
) -> Result<Vec<String>, Box<DiagnosticRecord>> {
    let mut found = Vec::new();
    walk(root, root, pattern, &mut found, origin)?;
    found.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    found.dedup();
    Ok(found)
}

fn walk(
    root: &Path,
    dir: &Path,
    rest: &[Segment],
    found: &mut Vec<String>,
    origin: &str,
) -> Result<(), Box<DiagnosticRecord>> {
    let Some(segment) = rest.first() else {
        return Ok(());
    };
    let further = &rest[1..];
    let accept = |path: &Path, found: &mut Vec<String>| {
        if path.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            found.push(portable(relative.as_os_str()));
        }
    };
    match segment {
        Segment::Literal(text) => {
            let next = dir.join(text);
            if further.is_empty() {
                accept(&next, found);
                Ok(())
            } else if next.is_dir() {
                walk(root, &next, further, found, origin)
            } else {
                Ok(())
            }
        }
        Segment::Doublestar => {
            walk(root, dir, further, found, origin)?;
            if further.is_empty() {
                for (child, _) in read_dir(dir, origin)? {
                    accept(&child, found);
                }
            }
            for (child, is_dir) in read_dir(dir, origin)? {
                if is_dir {
                    walk(root, &child, rest, found, origin)?;
                }
            }
            Ok(())
        }
        Segment::Pattern(text) => {
            for (child, is_dir) in read_dir(dir, origin)? {
                let Some(name) = child.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !matches_pattern(text, name) {
                    continue;
                }
                if further.is_empty() {
                    accept(&child, found);
                } else if is_dir {
                    walk(root, &child, further, found, origin)?;
                }
            }
            Ok(())
        }
    }
}

/// The entries of one directory, each with whether it is a directory
/// that is not a link.
fn read_dir(dir: &Path, origin: &str) -> Result<Vec<(PathBuf, bool)>, Box<DiagnosticRecord>> {
    let read = std::fs::read_dir(dir).map_err(|error| {
        let shown = dir.display().to_string();
        Box::new(record(
            SOURCE_UNREADABLE,
            &[&shown, &error.to_string()],
            origin,
        ))
    })?;
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        entries.push((entry.path(), is_dir));
    }
    Ok(entries)
}

/// Whether a path's segments match a pattern's.
fn matches(pattern: &[Segment], path: &[&str]) -> bool {
    match pattern.first() {
        None => path.is_empty(),
        Some(Segment::Doublestar) => (0..=path.len()).any(|at| matches(&pattern[1..], &path[at..])),
        Some(Segment::Literal(text)) => {
            path.first() == Some(&text.as_str()) && matches(&pattern[1..], &path[1..])
        }
        Some(Segment::Pattern(text)) => path
            .first()
            .is_some_and(|name| matches_pattern(text, name) && matches(&pattern[1..], &path[1..])),
    }
}

/// `{a,b}` alternatives: one flat, non-empty, balanced brace group per
/// occurrence, no nesting.
fn balanced_alternatives(part: &str) -> bool {
    let mut depth = 0usize;
    let mut run = String::new();
    for byte in part.bytes() {
        match byte {
            b'{' => {
                depth += 1;
                if depth > 1 {
                    return false;
                }
                run.clear();
            }
            b'}' => {
                if depth == 0 || run.is_empty() {
                    return false;
                }
                depth -= 1;
            }
            _ if depth == 1 => run.push(byte as char),
            _ => {}
        }
    }
    depth == 0
}

/// Whether one path segment matches a pattern segment: `*` within the
/// segment and flat `{a,b}` alternatives.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    alternatives(pattern)
        .iter()
        .any(|alternative| matches_star(alternative, name))
}

/// The brace alternatives of a pattern segment: the cross-product of its
/// (at most one, flat) brace group with its literal parts.
fn alternatives(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let Some(close) = pattern[open..].find('}').map(|at| open + at) else {
        return vec![pattern.to_owned()];
    };
    let head = &pattern[..open];
    let tail = &pattern[close + 1..];
    pattern[open + 1..close]
        .split(',')
        .map(|choice| format!("{head}{choice}{tail}"))
        .collect()
}

/// `*` matches any run of bytes within the segment, including none.
fn matches_star(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, rest)) => {
            if !name.starts_with(head) {
                return false;
            }
            let name = &name[head.len()..];
            if rest.is_empty() {
                return true;
            }
            (0..=name.len())
                .filter(|at| name.is_char_boundary(*at))
                .any(|at| matches_star(rest, &name[at..]))
        }
    }
}

/// Whether a path is a literal location in the local source: no
/// `$name/` and no pattern syntax. Dialect definitions import only these.
pub fn is_literal(path: &str) -> bool {
    !path.starts_with('$') && !path.contains(['*', '{', '}'])
}

/// The pattern syntax a rule admits as its declaration spells it.
pub fn pattern_names(patterns: &Patterns) -> Vec<&'static str> {
    let mut names = Vec::new();
    if patterns.star {
        names.push("star");
    }
    if patterns.doublestar {
        names.push("doublestar");
    }
    if patterns.braces {
        names.push("braces");
    }
    names
}

/// A locations rule as summaries and listings spell it: its patterns,
/// then whether an empty match is allowed.
pub(crate) fn describe(rule: &LocationsDecl) -> String {
    let allow = if rule.allow_empty { " allow-empty" } else { "" };
    format!("[{}]{allow}", pattern_names(&rule.patterns).join(","))
}

/// The locations rule definition documents declare for their own
/// imports, read from their syntax as each document is read, so a rule
/// written after the import it governs still governs it.
#[derive(Debug, Default)]
pub(crate) struct DeclaredLocations {
    rule: Option<LocationsDecl>,
}

impl DeclaredLocations {
    /// Record the rule one document's schema root declares; the first
    /// declared is kept, as the compiler reports any repeat. Text that
    /// does not parse declares nothing; binding reports it. Whether the
    /// document's root is a schema is returned: a schema definition's
    /// own locations are confined, a dialect definition's are not.
    pub(crate) fn scan(&mut self, text: &str) -> bool {
        let Ok(parsed) = ParsedDocument::parse("", text) else {
            return false;
        };
        let Some(root) = parsed.nodes().first().filter(|root| root.name == "schema") else {
            return false;
        };
        if self.rule.is_none()
            && let Some(node) = root.children.iter().find(|c| c.name == "locations")
        {
            self.rule = Some(declared_rule(node));
        }
        true
    }

    pub(crate) fn rule(&self) -> Option<&LocationsDecl> {
        self.rule.as_ref()
    }
}

fn declared_rule(node: &SyntaxNode) -> LocationsDecl {
    let patterns = node
        .children
        .iter()
        .find(|c| c.name == "patterns")
        .map_or_else(Patterns::default, |rules| Patterns {
            star: flag(rules, "star"),
            doublestar: flag(rules, "doublestar"),
            braces: flag(rules, "braces"),
        });
    LocationsDecl {
        patterns,
        allow_empty: flag(node, "allow-empty"),
        site: Site::new(0, node.span),
    }
}

/// A boolean property; the rightmost of a repeated key wins.
fn flag(node: &SyntaxNode, key: &str) -> bool {
    matches!(
        node.props
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, scalar)| &scalar.value),
        Some(Value::Bool(true))
    )
}

pub(crate) fn admit_tree(tree: &SourceTree) -> Result<Tree, String> {
    match tree {
        SourceTree::Dir(path) => canonical_dir(path).map(Tree::Dir),
        SourceTree::Embedded(files) => {
            let mut held = BTreeMap::new();
            for (key, text) in files {
                if key.is_empty() || normalize(key).as_deref() != Some(key.as_str()) {
                    return Err(format!(
                        "embedded file key {key:?} is not a normalized relative path"
                    ));
                }
                held.insert(key.clone(), Rc::from(text.as_str()));
            }
            Ok(Tree::Embedded(held))
        }
    }
}

pub(crate) fn canonical_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("directory {} does not resolve: {error}", path.display()))?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(format!("{} is not a directory", path.display()))
    }
}

/// `path` with `.` and empty segments dropped and each `..` taking back
/// the segment before it; `None` when a `..` would leave the root or the
/// path is absolute.
pub(crate) fn normalize(path: &str) -> Option<String> {
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

fn join(start: &str, rest: &str) -> String {
    if start.is_empty() {
        rest.to_owned()
    } else {
        format!("{start}/{rest}")
    }
}

fn parent(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The shown path of `relative` under `home`.
fn shown(home: &Home, relative: &str) -> String {
    match home {
        Home::Local(root) if root.shown.as_os_str().is_empty() => relative.to_owned(),
        Home::Local(root) => root.shown.join(relative).display().to_string(),
        Home::Named(name) => format!("${name}/{relative}"),
        Home::Unconfined => relative.to_owned(),
    }
}

fn home_name(home: &Home) -> String {
    match home {
        Home::Named(name) => format!("${name}"),
        Home::Local(root) if root.shown.as_os_str().is_empty() => "the project root".to_owned(),
        Home::Local(root) => format!("the directory {}", root.shown.display()),
        Home::Unconfined => "the project root".to_owned(),
    }
}

pub(crate) fn portable(path: &std::ffi::OsStr) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn is_source_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn unresolved(location: &str, reason: &str, origin: &str) -> Box<DiagnosticRecord> {
    Box::new(record(LOCATION_UNRESOLVED, &[location, reason], origin))
}
