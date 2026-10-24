//! The source set of one reading: registered and declared trees,
//! overrides applied by layer, origin trees located in the store on
//! first use; and the sources verb's reports. kdlith never fetches.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{SourceKind, SourceSet, SourceUse};
use kdlith_runtime::{stable_hash, stable_hash_step};
use serde_json::{Value as Json, json};

use crate::diag::{
    SOURCE_INTEGRITY, SOURCE_INVALID, SOURCE_NOT_MATERIALISED, SOURCE_OVERRIDDEN, SOURCE_UNPINNED,
    record,
};
use crate::digest::{files_digest, set_fingerprint, tree_digest};
use crate::locations::{
    LocalRoot, SourceTree, Sources, admit_tree, canonical_dir, is_source_name, portable,
};
use crate::manifest::{
    Declared, Layer, Override, SourceOrigin, Spot, describe_origin, origin_key, read_manifest,
    read_overrides,
};

/// A named source as a reading resolves against it.
pub(crate) enum Tree {
    /// The directory's canonical path.
    Dir(PathBuf),
    Embedded(BTreeMap<String, Rc<str>>),
    /// A tree the store holds, located on the source's first use.
    Origin(Rc<OriginTree>),
}

/// A located tree.
pub(crate) enum View<'t> {
    Dir(&'t Path),
    Embedded(&'t BTreeMap<String, Rc<str>>),
}

/// One named source of the set.
pub(crate) struct Entry {
    pub(crate) tree: Tree,
    /// Where the sources manifest declares it; `None` for a source the
    /// host registers.
    spot: Option<Spot>,
    /// The origin the manifest declares, kept when an override replaces
    /// its tree.
    origin: Option<Rc<OriginTree>>,
    overridden: Option<(Layer, String)>,
    /// The kind the source is declared as, kept when an override
    /// replaces its tree.
    declared: SourceKind,
    /// The identity of the tree a reading reads, computed on first ask.
    identity: OnceCell<Result<String, String>>,
}

/// An origin source's tree in the store.
pub(crate) struct OriginTree {
    origin: SourceOrigin,
    pinned: Option<String>,
    /// Its path under the store: `sha256/<hex>` when pinned, else
    /// `origin/<key>`.
    relative: String,
    store: Option<PathBuf>,
    located: OnceCell<Result<PathBuf, Box<DiagnosticRecord>>>,
}

impl OriginTree {
    fn new(origin: SourceOrigin, pinned: Option<String>, store: Option<&Path>) -> Self {
        let relative = match pinned.as_deref().and_then(|p| p.strip_prefix("sha256:")) {
            Some(hex) => format!("sha256/{hex}"),
            None => format!("origin/{}", origin_key(&origin)),
        };
        Self {
            origin,
            pinned,
            relative,
            store: store.map(Path::to_path_buf),
            located: OnceCell::new(),
        }
    }

    /// The store path as messages show it.
    fn target(&self) -> String {
        match &self.store {
            Some(store) => store.join(&self.relative).display().to_string(),
            None => format!("<store>/{}", self.relative),
        }
    }

    /// The tree's canonical directory, checked once against its pin:
    /// absent is `KDLITH-LOC006`, a digest other than the pin
    /// `KDLITH-LOC007`.
    fn locate(&self, name: &str, origin: &str) -> Result<&Path, Box<DiagnosticRecord>> {
        match self.located.get_or_init(|| self.check(name, origin)) {
            Ok(dir) => Ok(dir),
            Err(failure) => Err(failure.clone()),
        }
    }

    fn check(&self, name: &str, origin: &str) -> Result<PathBuf, Box<DiagnosticRecord>> {
        let target = self.target();
        let absent = |reason: &str| {
            let fetch = format!(
                "Fetch {} into {target}{}; kdlith sources plan lists every origin with its target",
                describe_origin(&self.origin),
                self.pinned
                    .as_deref()
                    .map(|pin| format!(" (its tree must digest to {pin})"))
                    .unwrap_or_default(),
            );
            Box::new(
                record(SOURCE_NOT_MATERIALISED, &[name, &target, reason], origin)
                    .with_correction(fetch),
            )
        };
        let Some(store) = &self.store else {
            return Err(absent("no store is given (--store DIR)"));
        };
        let dir = canonical_dir(&store.join(&self.relative))
            .map_err(|_| absent("the store holds no tree there"))?;
        if let Some(pinned) = &self.pinned {
            let digest = tree_digest(&dir).unwrap_or_else(|reason| format!("no digest ({reason})"));
            if &digest != pinned {
                return Err(Box::new(record(
                    SOURCE_INTEGRITY,
                    &[name, pinned, &target, &digest],
                    origin,
                )));
            }
        }
        Ok(dir)
    }
}

/// What a reading's source set is made from.
pub(crate) struct SourceInputs<'a> {
    pub(crate) root: Option<&'a Path>,
    pub(crate) registered: &'a [(String, SourceTree)],
    pub(crate) manifest: Option<&'a Path>,
    pub(crate) overlay: Option<&'a Path>,
    pub(crate) overrides: &'a [(String, PathBuf)],
    pub(crate) store: Option<&'a Path>,
}

impl Sources {
    /// Validate the project root, the registered sources, the sources
    /// manifest, and every override, each failure reported before any
    /// document is read (`KDLITH-LOC005`); then apply the overrides, the
    /// most local layer winning, each reported (`KDLITH-LOC009`) among
    /// the returned warnings.
    pub(crate) fn admit(
        inputs: &SourceInputs<'_>,
        origin: &str,
    ) -> Result<(Self, Vec<DiagnosticRecord>), Vec<DiagnosticRecord>> {
        let mut failures = Vec::new();
        let spelled = inputs
            .root
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let canonical = match canonical_dir(&spelled) {
            Ok(canonical) => Some(canonical),
            Err(reason) => {
                let subject = "(project root)";
                failures.push(record(SOURCE_INVALID, &[subject, &reason], origin));
                None
            }
        };
        let mut named: BTreeMap<String, Entry> = BTreeMap::new();
        for (name, tree) in inputs.registered {
            let reason = if !is_source_name(name) {
                Some("the name is not a non-empty kebab name".to_owned())
            } else if named.contains_key(name) {
                Some("the source is registered more than once".to_owned())
            } else {
                match admit_tree(tree) {
                    Ok(tree) => {
                        named.insert(name.clone(), Entry::new(tree, None, None));
                        None
                    }
                    Err(reason) => Some(reason),
                }
            };
            if let Some(reason) = reason {
                failures.push(record(SOURCE_INVALID, &[name, &reason], origin));
            }
        }
        let mut overrides: Vec<Override> = Vec::new();
        if let Some(path) = inputs.manifest {
            match read_manifest(path, origin) {
                Ok(manifest) => {
                    for decl in manifest.sources {
                        let entry = match named.get(&decl.name) {
                            Some(entry) if entry.spot.is_none() => {
                                Err("the host registers this name; replace it with an override"
                                    .to_owned())
                            }
                            Some(_) => Err("the source is declared more than once".to_owned()),
                            None => match decl.declared {
                                Declared::Dir(dir) => canonical_dir(&dir).map(|dir| {
                                    Entry::new(Tree::Dir(dir), Some(decl.spot.clone()), None)
                                }),
                                Declared::Origin { origin, pinned } => {
                                    let tree =
                                        Rc::new(OriginTree::new(origin, pinned, inputs.store));
                                    Ok(Entry::new(
                                        Tree::Origin(tree.clone()),
                                        Some(decl.spot.clone()),
                                        Some(tree),
                                    ))
                                }
                            },
                        };
                        match entry {
                            Ok(entry) => {
                                named.insert(decl.name, entry);
                            }
                            Err(reason) => failures.push(decl.spot.locate(record(
                                SOURCE_INVALID,
                                &[&decl.name, &reason],
                                origin,
                            ))),
                        }
                    }
                    overrides.extend(manifest.overrides);
                }
                Err(records) => failures.extend(records),
            }
        }
        if let Some(path) = inputs.overlay {
            match read_overrides(path, origin) {
                Ok(found) => overrides.extend(found),
                Err(records) => failures.extend(records),
            }
        }
        overrides.extend(inputs.overrides.iter().map(|(name, dir)| Override {
            name: name.clone(),
            dir: dir.clone(),
            written: dir.display().to_string(),
            layer: Layer::Cli,
            spot: None,
        }));
        let mut layered: BTreeMap<String, Vec<(Override, PathBuf)>> = BTreeMap::new();
        let mut seen = HashSet::new();
        for found in overrides {
            let reason = if !is_source_name(&found.name) {
                Err("the name is not a non-empty kebab name".to_owned())
            } else if !named.contains_key(&found.name) {
                Err("the override names no declared source; declare it with source".to_owned())
            } else if !seen.insert((found.name.clone(), found.layer)) {
                Err(format!(
                    "the source is overridden more than once in the {} layer",
                    found.layer.name()
                ))
            } else {
                canonical_dir(&found.dir)
            };
            match reason {
                Ok(dir) => layered
                    .entry(found.name.clone())
                    .or_default()
                    .push((found, dir)),
                Err(reason) => {
                    let failure = record(SOURCE_INVALID, &[&found.name, &reason], origin);
                    failures.push(match &found.spot {
                        Some(spot) => spot.locate(failure),
                        None => failure,
                    });
                }
            }
        }
        let Some(canonical) = canonical.filter(|_| failures.is_empty()) else {
            return Err(failures);
        };
        let mut warnings = Vec::new();
        for (name, mut found) in layered {
            found.sort_by(|a, b| b.0.layer.cmp(&a.0.layer));
            let mut found = found.into_iter();
            let Some((winner, dir)) = found.next() else {
                continue;
            };
            let shadowed: Vec<Override> = found.map(|(shadowed, _)| shadowed).collect();
            let Some(entry) = named.get_mut(&name) else {
                continue;
            };
            let declared = entry.identity();
            entry.tree = Tree::Dir(dir);
            entry.overridden = Some((winner.layer, winner.written.clone()));
            warnings.push(overridden(&name, &winner, &declared, &shadowed, origin));
        }
        Ok((
            Self {
                fingerprint: fingerprint(&canonical, &named),
                root: Rc::new(LocalRoot::project(canonical)),
                named,
                used: RefCell::new(BTreeSet::new()),
            },
            warnings,
        ))
    }

    /// Whether the set holds a source called `name`.
    pub(crate) fn holds(&self, name: &str) -> bool {
        self.named.contains_key(name)
    }

    /// Locate the source `name` for use: an origin tree is found in the
    /// store and checked against its pin on its first use. Each use is
    /// recorded for the unpinned-source notes.
    pub(crate) fn enter(&self, name: &str, origin: &str) -> Result<(), Box<DiagnosticRecord>> {
        let Some(entry) = self.named.get(name) else {
            return Ok(());
        };
        if let Tree::Origin(tree) = &entry.tree {
            tree.locate(name, origin)?;
        }
        self.used.borrow_mut().insert(name.to_owned());
        Ok(())
    }

    /// The tree backing `name`, once located.
    pub(crate) fn view(&self, name: &str) -> Option<View<'_>> {
        match &self.named.get(name)?.tree {
            Tree::Dir(root) => Some(View::Dir(root)),
            Tree::Embedded(files) => Some(View::Embedded(files)),
            Tree::Origin(tree) => match tree.located.get()? {
                Ok(dir) => Some(View::Dir(dir)),
                Err(_) => None,
            },
        }
    }

    /// One `KDLITH-LOC010` note per unpinned dir or origin source of the
    /// manifest the reading used, in name order, each with the tree's
    /// digest. Sources the host registers and overridden sources are
    /// exempt; an override is already a warning.
    pub(crate) fn unpinned_notes(&self, origin: &str) -> Vec<DiagnosticRecord> {
        let used = self.used.borrow();
        used.iter()
            .filter_map(|name| {
                let entry = self.named.get(name)?;
                let spot = entry.spot.as_ref()?;
                if entry.overridden.is_some() {
                    return None;
                }
                let kind = match &entry.tree {
                    Tree::Dir(_) => "dir",
                    Tree::Origin(tree) if tree.pinned.is_none() => {
                        tree.located.get()?.as_ref().ok()?;
                        "origin"
                    }
                    _ => return None,
                };
                let digest = entry
                    .read_identity()
                    .map_or_else(|reason| format!("none ({reason})"), str::to_owned);
                let note = record(SOURCE_UNPINNED, &[name, kind, &digest], origin)
                    .with_fact("source", json!(name))
                    .with_fact("kind", json!(kind))
                    .with_fact("digest", json!(digest));
                Some(spot.locate(note))
            })
            .collect()
    }
}

impl Sources {
    /// Each named source the reading used, by name, with the identity of
    /// the tree it read (`none` when the tree can have none).
    pub(crate) fn used_identities(&self) -> BTreeMap<String, String> {
        let used = self.used.borrow();
        used.iter()
            .filter_map(|name| {
                let entry = self.named.get(name)?;
                let identity = entry.read_identity().unwrap_or("none");
                Some((name.clone(), identity.to_owned()))
            })
            .collect()
    }

    /// The source-set fingerprint of the named sources the reading used;
    /// `None` when it used none.
    pub(crate) fn used_fingerprint(&self) -> Option<String> {
        let used = self.used_identities();
        (!used.is_empty()).then(|| {
            set_fingerprint(
                used.iter()
                    .map(|(name, identity)| (name.as_str(), identity.as_str())),
            )
        })
    }

    /// The identity source `name` reads now, entering it as a reading
    /// would; `None` when the set holds no such source or it cannot be
    /// entered.
    pub(crate) fn identity_of(&self, name: &str, origin: &str) -> Option<String> {
        let entry = self.named.get(name)?;
        self.enter(name, origin).ok()?;
        Some(entry.read_identity().unwrap_or("none").to_owned())
    }

    /// The reading's sources item: the local source first when the
    /// reading read a document from it, with `local` the identity of the
    /// documents it read, then every named source it used, by name.
    pub(crate) fn source_set(&self, local: Option<String>) -> SourceSet {
        let mut sources = Vec::new();
        if let Some(identity) = local {
            sources.push(SourceUse::new(None, SourceKind::Local, Some(identity)));
        }
        let used = self.used.borrow();
        for name in used.iter() {
            if let Some(entry) = self.named.get(name) {
                sources.push(entry.usage(name));
            }
        }
        drop(used);
        SourceSet::new(sources, self.used_fingerprint())
    }
}

impl Entry {
    fn new(tree: Tree, spot: Option<Spot>, origin: Option<Rc<OriginTree>>) -> Self {
        let declared = match (&origin, &tree) {
            (Some(_), _) => SourceKind::Origin,
            (None, Tree::Embedded(_)) => SourceKind::Embedded,
            _ => SourceKind::Dir,
        };
        Self {
            tree,
            spot,
            origin,
            overridden: None,
            declared,
            identity: OnceCell::new(),
        }
    }

    /// The identity of the tree a reading reads: an origin's pin once
    /// its tree is checked against it, else the tree's canonical digest
    /// (an embedded source's is its build digest). An origin the store
    /// does not hold has none.
    fn read_identity(&self) -> Result<&str, &str> {
        let identity = self.identity.get_or_init(|| match &self.tree {
            Tree::Dir(dir) => tree_digest(dir),
            Tree::Embedded(files) => files_digest(
                files
                    .iter()
                    .map(|(key, text)| (key.as_str(), text.as_bytes())),
            ),
            Tree::Origin(tree) => match tree.located.get() {
                Some(Ok(dir)) => match &tree.pinned {
                    Some(pinned) => Ok(pinned.clone()),
                    None => tree_digest(dir),
                },
                _ => Err("the store holds no checked tree".to_owned()),
            },
        });
        identity.as_deref().map_err(String::as_str)
    }

    /// The entry as the reading's sources item reports it.
    fn usage(&self, name: &str) -> SourceUse {
        let pinned = self.overridden.is_none()
            && match &self.tree {
                Tree::Embedded(_) => true,
                Tree::Origin(tree) => tree.pinned.is_some(),
                Tree::Dir(_) => false,
            };
        let identity = self.read_identity().ok().map(str::to_owned);
        let mut usage =
            SourceUse::new(Some(name.to_owned()), self.declared, identity).pinned(pinned);
        if let Some(origin) = &self.origin {
            usage = usage.with_origin(origin.origin.clone());
        }
        if let Some((layer, directory)) = &self.overridden {
            usage = usage.overridden_by(layer.name(), directory.clone());
        }
        usage
    }

    /// What an override displaces, as `KDLITH-LOC009` names it.
    fn identity(&self) -> String {
        match &self.tree {
            Tree::Embedded(files) => {
                let files = files
                    .iter()
                    .map(|(key, text)| (key.as_str(), text.as_bytes()));
                match files_digest(files) {
                    Ok(digest) => format!("embedded {digest}"),
                    Err(_) => "embedded".to_owned(),
                }
            }
            Tree::Origin(tree) => match &tree.pinned {
                Some(pinned) => format!("pinned {pinned}"),
                None => "unpinned".to_owned(),
            },
            Tree::Dir(_) => "unpinned".to_owned(),
        }
    }

    fn kind(&self) -> &'static str {
        match (&self.origin, &self.tree) {
            (Some(_), _) => "origin",
            (None, Tree::Embedded(_)) => "embedded",
            _ => "dir",
        }
    }

    /// The digest of the tree a reading reads: the override's, else the
    /// declared tree's; `None` for an origin the store does not hold.
    fn digest(&self) -> Option<Result<String, String>> {
        Some(match &self.tree {
            Tree::Dir(dir) => tree_digest(dir),
            Tree::Embedded(files) => files_digest(
                files
                    .iter()
                    .map(|(key, text)| (key.as_str(), text.as_bytes())),
            ),
            Tree::Origin(tree) => {
                let store = tree.store.as_ref()?;
                let dir = canonical_dir(&store.join(&tree.relative)).ok()?;
                tree_digest(&dir)
            }
        })
    }
}

/// The `KDLITH-LOC009` warning of one applied override.
fn overridden(
    name: &str,
    winner: &Override,
    declared: &str,
    shadowed: &[Override],
    origin: &str,
) -> DiagnosticRecord {
    let displaced = if shadowed.is_empty() {
        String::new()
    } else {
        let each: Vec<String> = shadowed
            .iter()
            .map(|o| format!("the {} override {}", o.layer.name(), o.written))
            .collect();
        format!("; it displaces {}", each.join(" and "))
    };
    let warning = record(
        SOURCE_OVERRIDDEN,
        &[
            name,
            &winner.written,
            winner.layer.name(),
            declared,
            &displaced,
        ],
        origin,
    )
    .with_fact("source", json!(name))
    .with_fact("layer", json!(winner.layer.name()))
    .with_fact("declared", json!(declared))
    .with_fact("directory", json!(winner.written))
    .with_fact(
        "shadowed",
        Json::Array(
            shadowed
                .iter()
                .map(|o| json!({"layer": o.layer.name(), "directory": o.written}))
                .collect(),
        ),
    );
    match &winner.spot {
        Some(spot) => spot.locate(warning),
        None => warning,
    }
}

fn fingerprint(root: &Path, named: &BTreeMap<String, Entry>) -> u64 {
    let mut hash = stable_hash(portable(root.as_os_str()).as_bytes());
    for (name, entry) in named {
        hash = stable_hash_step(hash, b"\0");
        hash = stable_hash_step(hash, name.as_bytes());
        match &entry.tree {
            Tree::Dir(path) => {
                hash = stable_hash_step(hash, b"\0dir\0");
                hash = stable_hash_step(hash, portable(path.as_os_str()).as_bytes());
            }
            Tree::Embedded(files) => {
                hash = stable_hash_step(hash, b"\0embedded");
                for (key, text) in files {
                    hash = stable_hash_step(hash, b"\0");
                    hash = stable_hash_step(hash, key.as_bytes());
                    let length = u64::try_from(text.len()).unwrap_or(u64::MAX);
                    hash = stable_hash_step(hash, &length.to_le_bytes());
                    hash = stable_hash_step(hash, text.as_bytes());
                }
            }
            Tree::Origin(tree) => {
                hash = stable_hash_step(hash, b"\0origin\0");
                hash = stable_hash_step(hash, tree.target().as_bytes());
            }
        }
    }
    hash
}

/// What one `kdlith sources` subcommand reports: its lines, and the
/// diagnostics deciding its exit.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct SourcesReport {
    pub lines: Vec<Json>,
    pub records: Vec<DiagnosticRecord>,
}

/// The fetch plan: every origin the sources manifest declares, in name
/// order, with its origin, its pin or `false`, its path under the
/// store, and whether the store holds a tree there.
pub fn plan_sources(options: &crate::ReadOptions) -> Result<SourcesReport, Vec<DiagnosticRecord>> {
    let (sources, warnings) = Sources::admit(&options.source_inputs(), ORIGIN)?;
    let lines = origins(&sources)
        .map(|(name, tree)| {
            let materialised = tree
                .store
                .as_ref()
                .is_some_and(|store| store.join(&tree.relative).is_dir());
            json!({
                "source": name,
                "origin": tree.origin,
                "pinned": pin(tree),
                "target": tree.relative,
                "materialised": materialised,
            })
        })
        .collect();
    Ok(SourcesReport {
        lines,
        records: warnings,
    })
}

/// Re-hash every origin tree in the store: one line per origin, in name
/// order, with its status — `verified`, `mismatch` (`KDLITH-LOC007`),
/// `unpinned` (materialised, with no pin to check), or `absent`
/// (`KDLITH-LOC006`).
pub fn verify_sources(
    options: &crate::ReadOptions,
) -> Result<SourcesReport, Vec<DiagnosticRecord>> {
    let (sources, mut records) = Sources::admit(&options.source_inputs(), ORIGIN)?;
    let mut lines = Vec::new();
    for (name, tree) in origins(&sources) {
        let checked = OriginTree::new(
            tree.origin.clone(),
            tree.pinned.clone(),
            tree.store.as_deref(),
        );
        let (status, digest) = match checked.check(name, ORIGIN) {
            Ok(dir) => {
                let digest = tree_digest(&dir).ok();
                (
                    if tree.pinned.is_some() {
                        "verified"
                    } else {
                        "unpinned"
                    },
                    digest,
                )
            }
            Err(failure) => {
                let mismatch = failure.code == SOURCE_INTEGRITY;
                let digest = tree
                    .store
                    .as_ref()
                    .filter(|_| mismatch)
                    .and_then(|store| tree_digest(&store.join(&tree.relative)).ok());
                records.push(*failure);
                (if mismatch { "mismatch" } else { "absent" }, digest)
            }
        };
        lines.push(json!({
            "source": name,
            "target": tree.relative,
            "pinned": pin(tree),
            "status": status,
            "digest": digest,
        }));
    }
    Ok(SourcesReport { lines, records })
}

/// Every source of the set, in name order: its kind, who declares it,
/// its pin or `false`, the digest of the tree a reading would read (the
/// pinning aid; `null` for an origin the store does not hold), and the
/// override replacing it, if any.
pub fn list_sources(options: &crate::ReadOptions) -> Result<SourcesReport, Vec<DiagnosticRecord>> {
    let (sources, mut records) = Sources::admit(&options.source_inputs(), ORIGIN)?;
    let mut lines = Vec::new();
    for (name, entry) in &sources.named {
        let digest = match entry.digest() {
            Some(Ok(digest)) => Json::String(digest),
            Some(Err(reason)) => {
                records.push(record(
                    SOURCE_INVALID,
                    &[name, &format!("its tree has no digest: {reason}")],
                    ORIGIN,
                ));
                Json::Null
            }
            None => Json::Null,
        };
        let pinned = match (&entry.origin, &entry.tree) {
            (Some(tree), _) => pin(tree),
            (None, Tree::Embedded(_)) => digest.clone(),
            _ => Json::Bool(false),
        };
        let overridden = match &entry.overridden {
            Some((layer, directory)) => json!({"layer": layer.name(), "directory": directory}),
            None => Json::Bool(false),
        };
        lines.push(json!({
            "source": name,
            "kind": entry.kind(),
            "declared": if entry.spot.is_some() { "manifest" } else { "host" },
            "pinned": pinned,
            "digest": digest,
            "overridden": overridden,
        }));
    }
    Ok(SourcesReport { lines, records })
}

const ORIGIN: &str = "sources-manifest";

fn origins(sources: &Sources) -> impl Iterator<Item = (&String, &OriginTree)> {
    sources
        .named
        .iter()
        .filter_map(|(name, entry)| Some((name, entry.origin.as_deref()?)))
}

fn pin(tree: &OriginTree) -> Json {
    match &tree.pinned {
        Some(pinned) => Json::String(pinned.clone()),
        None => Json::Bool(false),
    }
}
