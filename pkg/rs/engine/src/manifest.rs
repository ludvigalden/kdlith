//! Sources manifests and overrides files: the documents naming which
//! tree backs each source, read only when a host names them. Their
//! shape is the `kdlith-sources` schema (`pkg/kdl/schemas/sources/`).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::Value;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diag::{SOURCE_INVALID, located, record};
use crate::digest::{hex, is_digest};
use crate::documents::read_text;
use crate::locations::{is_source_name, normalize};
use crate::syntax::{ParsedDocument, SyntaxNode};

/// Where an origin source's tree comes from: a git repository, the
/// revision, and the directory in it. kdlith never fetches; the origin
/// says what to fetch into the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceOrigin {
    pub git: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

impl SourceOrigin {
    /// The store key of the origin when unpinned: the SHA-256 of its
    /// properties in name order, each written `NAME=VALUE` and a
    /// newline, the path normalized and omitted when it names the
    /// repository root.
    pub(crate) fn key(&self) -> String {
        let mut text = format!("git={}\n", self.git);
        if let Some(path) = self.path.as_deref().filter(|path| !path.is_empty()) {
            text.push_str(&format!("path={path}\n"));
        }
        if let Some(rev) = &self.rev {
            text.push_str(&format!("rev={rev}\n"));
        }
        hex(&Sha256::digest(text.as_bytes()))
    }

    /// The origin as a fetch instruction spells it.
    pub(crate) fn describe(&self) -> String {
        format!(
            "git {} at {}, directory {}",
            self.git,
            self.rev.as_deref().unwrap_or("any revision"),
            self.path.as_deref().unwrap_or("the repository root"),
        )
    }
}

/// What a sources manifest declares a source to be.
pub(crate) enum Declared {
    /// A directory, relative to the manifest.
    Dir(PathBuf),
    Origin {
        origin: SourceOrigin,
        pinned: Option<String>,
    },
}

/// One source a sources manifest declares.
pub(crate) struct Declaration {
    pub(crate) name: String,
    pub(crate) declared: Declared,
    pub(crate) spot: Spot,
}

/// Where an override is written; the most local layer wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Layer {
    Manifest,
    Overlay,
    Cli,
}

impl Layer {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Layer::Manifest => "manifest",
            Layer::Overlay => "overlay",
            Layer::Cli => "cli",
        }
    }
}

/// One override: a directory replacing a declared source.
pub(crate) struct Override {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
    /// The directory as written.
    pub(crate) written: String,
    pub(crate) layer: Layer,
    /// Where a file writes it; `None` on the command line.
    pub(crate) spot: Option<Spot>,
}

/// A node of a manifest or overrides file, so a record locates there.
#[derive(Clone)]
pub(crate) struct Spot {
    document: Rc<ParsedDocument>,
    span: ByteSpan,
}

impl Spot {
    pub(crate) fn locate(&self, record: DiagnosticRecord) -> DiagnosticRecord {
        located(record, &self.document, self.span)
    }
}

/// What a sources manifest declares: its sources and its own overrides.
pub(crate) struct Manifest {
    pub(crate) sources: Vec<Declaration>,
    pub(crate) overrides: Vec<Override>,
}

/// Read the sources manifest at `path`: one `sources` node holding
/// `source` and `override` nodes.
pub(crate) fn read_manifest(path: &Path, origin: &str) -> Result<Manifest, Vec<DiagnosticRecord>> {
    let mut reader = FileReader::open(path, Layer::Manifest, origin)?;
    reader.root("sources", true);
    reader.finish().map(|reader| Manifest {
        sources: reader.sources,
        overrides: reader.overrides,
    })
}

/// Read the overrides file at `path`: one `overrides` node holding
/// `override` nodes.
pub(crate) fn read_overrides(
    path: &Path,
    origin: &str,
) -> Result<Vec<Override>, Vec<DiagnosticRecord>> {
    let mut reader = FileReader::open(path, Layer::Overlay, origin)?;
    reader.root("overrides", false);
    reader.finish().map(|reader| reader.overrides)
}

struct FileReader<'o> {
    document: Rc<ParsedDocument>,
    /// The directory paths in the file resolve from.
    base: PathBuf,
    layer: Layer,
    origin: &'o str,
    failures: Vec<DiagnosticRecord>,
    sources: Vec<Declaration>,
    overrides: Vec<Override>,
}

impl<'o> FileReader<'o> {
    fn open(path: &Path, layer: Layer, origin: &'o str) -> Result<Self, Vec<DiagnosticRecord>> {
        let shown = path.display().to_string();
        let text = read_text(path, &shown, "document size", origin).map_err(|r| vec![*r])?;
        let document = ParsedDocument::parse(shown, &text).map_err(|r| vec![*r])?;
        Ok(Self {
            document: Rc::new(document),
            base: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            layer,
            origin,
            failures: Vec::new(),
            sources: Vec::new(),
            overrides: Vec::new(),
        })
    }

    fn finish(self) -> Result<Self, Vec<DiagnosticRecord>> {
        if self.failures.is_empty() {
            Ok(self)
        } else {
            Err(self.failures)
        }
    }

    fn spot(&self, span: ByteSpan) -> Spot {
        Spot {
            document: self.document.clone(),
            span,
        }
    }

    fn fail(&mut self, span: ByteSpan, subject: &str, reason: &str) {
        let failure = record(SOURCE_INVALID, &[subject, reason], self.origin);
        self.failures.push(self.spot(span).locate(failure));
    }

    /// The one root node `name`, holding the file's declarations.
    fn root(&mut self, name: &str, admits_sources: bool) {
        let document = self.document.clone();
        let subject = document.path().to_owned();
        match document.nodes() {
            [root] if root.name == name => {
                if root.annotated || !root.args.is_empty() || !root.props.is_empty() {
                    let reason = format!("the {name} node takes no values");
                    self.fail(root.span, &subject, &reason);
                }
                for node in &root.children {
                    self.declaration(node, admits_sources);
                }
            }
            nodes => {
                let span = nodes.first().map_or(ByteSpan::new(0, 0), |n| n.span);
                let reason = format!("the document holds one {name} node and nothing else");
                self.fail(span, &subject, &reason);
            }
        }
    }

    fn declaration(&mut self, node: &SyntaxNode, admits_sources: bool) {
        match node.name.as_str() {
            "source" if admits_sources => self.source(node),
            "source" => self.fail(
                node.span,
                "source",
                "an overrides file declares no sources; declare it in the sources manifest",
            ),
            "override" => self.override_node(node),
            other => {
                let reason = format!("{other} is not a source or an override");
                self.fail(node.span, other, &reason);
            }
        }
    }

    /// The name a `source` or `override` node gives as its one argument.
    fn name(&mut self, node: &SyntaxNode) -> Option<String> {
        match node.args.as_slice() {
            [scalar] if !scalar.annotated && !node.annotated => match &scalar.value {
                Value::String(name) if is_source_name(name) => return Some(name.clone()),
                Value::String(name) => {
                    self.fail(node.span, name, "the name is not a non-empty kebab name");
                    return None;
                }
                _ => {}
            },
            _ => {}
        }
        let reason = format!("a {} node takes one name and no annotation", node.name);
        self.fail(node.span, &node.name, &reason);
        None
    }

    /// The node's string properties by key; a repeated, annotated,
    /// non-string, or unadmitted property fails.
    fn props(
        &mut self,
        node: &SyntaxNode,
        subject: &str,
        admitted: &[&str],
    ) -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = Vec::new();
        for (key, scalar) in &node.props {
            let reason = if !admitted.contains(&key.as_str()) {
                Some(format!("{} takes no {key}= property", node.name))
            } else if found.iter().any(|(seen, _)| seen == key) {
                Some(format!("{key}= is given more than once"))
            } else {
                match &scalar.value {
                    Value::String(text) if !scalar.annotated => {
                        found.push((key.clone(), text.clone()));
                        None
                    }
                    _ => Some(format!("{key}= takes a string")),
                }
            };
            if let Some(reason) = reason {
                self.fail(node.span, subject, &reason);
            }
        }
        found
    }

    fn source(&mut self, node: &SyntaxNode) {
        let Some(name) = self.name(node) else {
            return;
        };
        let props = self.props(node, &name, &["dir"]);
        if let Some((_, dir)) = props.into_iter().next() {
            if !node.children.is_empty() {
                self.fail(
                    node.span,
                    &name,
                    "a dir source takes no children; only an origin source is pinned",
                );
                return;
            }
            self.sources.push(Declaration {
                name,
                declared: Declared::Dir(self.base.join(dir)),
                spot: self.spot(node.span),
            });
            return;
        }
        let mut origin = None;
        let mut pinned = None;
        let mut failed = false;
        for child in &node.children {
            match child.name.as_str() {
                "origin" if origin.is_none() => {
                    origin = self.origin(child, &name);
                    failed |= origin.is_none();
                }
                "pinned" if pinned.is_none() => {
                    pinned = self.pinned(child, &name);
                    failed |= pinned.is_none();
                }
                "origin" | "pinned" => {
                    let reason = format!("{} is given more than once", child.name);
                    self.fail(child.span, &name, &reason);
                    failed = true;
                }
                other => {
                    let reason = format!("a source takes origin and pinned, not {other}");
                    self.fail(child.span, &name, &reason);
                    failed = true;
                }
            }
        }
        match origin {
            Some(origin) if !failed => self.sources.push(Declaration {
                name,
                declared: Declared::Origin { origin, pinned },
                spot: self.spot(node.span),
            }),
            None if !failed => {
                self.fail(node.span, &name, "a source takes dir= or an origin child")
            }
            _ => {}
        }
    }

    fn origin(&mut self, node: &SyntaxNode, name: &str) -> Option<SourceOrigin> {
        if node.annotated || !node.args.is_empty() || !node.children.is_empty() {
            self.fail(node.span, name, "origin takes only git=, rev=, and path=");
            return None;
        }
        let props = self.props(node, name, &["git", "rev", "path"]);
        let get = |key: &str| {
            props
                .iter()
                .find(|(found, _)| found == key)
                .map(|(_, value)| value.clone())
        };
        let Some(git) = get("git").filter(|git| !git.is_empty()) else {
            self.fail(node.span, name, "origin takes a non-empty git=");
            return None;
        };
        let path = match get("path") {
            Some(path) => match normalize(&path) {
                Some(path) => Some(path),
                None => {
                    self.fail(
                        node.span,
                        name,
                        "origin path= is a relative path that stays inside the repository",
                    );
                    return None;
                }
            },
            None => None,
        };
        Some(SourceOrigin {
            git,
            path,
            rev: get("rev"),
        })
    }

    fn pinned(&mut self, node: &SyntaxNode, name: &str) -> Option<String> {
        let digest = match node.args.as_slice() {
            [scalar] if !scalar.annotated => match &scalar.value {
                Value::String(digest) if is_digest(digest) => Some(digest.clone()),
                _ => None,
            },
            _ => None,
        };
        if digest.is_none() || node.annotated || !node.props.is_empty() || !node.children.is_empty()
        {
            self.fail(
                node.span,
                name,
                "pinned takes one digest, sha256: and 64 lowercase hex digits",
            );
            return None;
        }
        digest
    }

    fn override_node(&mut self, node: &SyntaxNode) {
        let Some(name) = self.name(node) else {
            return;
        };
        let pin = "an override carries no pin; change the pin on its source";
        if node.props.iter().any(|(key, _)| key == "pinned")
            || node.children.iter().any(|child| child.name == "pinned")
        {
            self.fail(node.span, &name, pin);
            return;
        }
        if !node.children.is_empty() {
            self.fail(node.span, &name, "an override takes only dir=");
            return;
        }
        let props = self.props(node, &name, &["dir"]);
        let Some((_, written)) = props.into_iter().next() else {
            self.fail(node.span, &name, "an override takes dir=");
            return;
        };
        self.overrides.push(Override {
            name,
            dir: self.base.join(&written),
            written,
            layer: self.layer,
            spot: Some(self.spot(node.span)),
        });
    }
}
