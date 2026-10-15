//! Host-supplied discovery anchors: the one expansion every reading
//! path runs for grammar-admitted import expressions, the rejection of
//! an anchor no grammar declares, and the grammars definition documents
//! declare for their own imports.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::Value;

use crate::definition::{AnchorDecl, Definition, DiscoveryDecl, Patterns};
use crate::diag::{IMPORT_ANCHOR, IMPORT_GRAMMAR, IMPORT_UNRESOLVED, Site, record};
use crate::discovery::{self, ExpansionError};
use crate::names::{Names, Sym};
use crate::syntax::{ParsedDocument, SyntaxNode};

/// One import target: the canonical path the invocation admits and the
/// path as the reading shows it.
pub(crate) struct Target {
    pub(crate) resolved: PathBuf,
    pub(crate) shown: String,
}

/// The supplied anchors whose names `names` knows, keyed by symbol: a
/// grammar's own declarations decide what may resolve.
pub(crate) fn anchor_map(supplied: &[(String, PathBuf)], names: &Names) -> BTreeMap<Sym, PathBuf> {
    let mut anchors = BTreeMap::new();
    for (name, path) in supplied {
        if let Some(sym) = names.get(name) {
            anchors.insert(sym, path.clone());
        }
    }
    anchors
}

/// One `KDLITH-IMP005` per supplied anchor no grammar in `grammars`
/// declares, each name once, in supply order: an anchor the host passes
/// is never silently ignored.
pub(crate) fn undeclared(
    supplied: &[(String, PathBuf)],
    grammars: &[DiscoveryDecl],
    names: &Names,
    origin: &str,
) -> Vec<DiagnosticRecord> {
    let declared = |name: &str| {
        grammars
            .iter()
            .flat_map(|grammar| &grammar.anchors)
            .any(|anchor| names.resolve(anchor.name) == name)
    };
    let mut missing: Vec<&str> = Vec::new();
    for (name, _) in supplied {
        if !declared(name) && !missing.contains(&name.as_str()) {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        return Vec::new();
    }
    let listed: Vec<&str> = grammars.iter().map(|g| names.resolve(g.name)).collect();
    let grammar = if listed.is_empty() {
        "(none declared)".to_owned()
    } else {
        listed.join(", ")
    };
    missing
        .into_iter()
        .map(|name| record(IMPORT_ANCHOR, &[name, &grammar], origin))
        .collect()
}

/// Where an expression without a leading anchor is rooted: the
/// importing document's directory, and that directory as the reading
/// shows it.
pub(crate) struct Base<'p> {
    pub(crate) dir: &'p Path,
    pub(crate) shown: &'p Path,
}

/// The targets a grammar-admitted import expression expands to under
/// `grammar`, in portable-path-byte order. Zero matches do not resolve
/// unless the grammar allows an empty match.
pub(crate) fn expression_targets(
    expression: &str,
    grammar: Option<&DiscoveryDecl>,
    names: &Names,
    anchors: &BTreeMap<Sym, PathBuf>,
    base: &Base<'_>,
    origin: &str,
) -> Result<Vec<Target>, Box<DiagnosticRecord>> {
    let failed = |error: ExpansionError| Box::new(expansion_record(expression, &error, origin));
    let expansion = discovery::parse(expression, grammar, names).map_err(failed)?;
    let matches = discovery::expand(&expansion, names, anchors, base.dir).map_err(failed)?;
    if matches.is_empty() && !grammar.is_some_and(|grammar| grammar.allow_empty) {
        return Err(Box::new(record(IMPORT_UNRESOLVED, &[expression], origin)));
    }
    let mut targets = Vec::with_capacity(matches.len());
    for matched in matches {
        // A match under the importer's directory shows relative to the
        // importer as shown; an anchored match shows as its anchor does.
        let shown = match matched.strip_prefix(base.dir) {
            Ok(relative) => base.shown.join(relative),
            Err(_) => matched.clone(),
        };
        let shown = shown.display().to_string();
        let resolved = std::fs::canonicalize(&matched).map_err(|error| {
            let reason = error.to_string();
            Box::new(record(IMPORT_UNRESOLVED, &[&shown, &reason], origin))
        })?;
        targets.push(Target { resolved, shown });
    }
    Ok(targets)
}

/// An import-expression failure as its registered diagnostic, with the
/// values the code's message renders: the mapping the compiler reports.
fn expansion_record(expression: &str, error: &ExpansionError, origin: &str) -> DiagnosticRecord {
    let [first, second] = error.values();
    match error {
        ExpansionError::NotAdmitted { .. } => {
            record(IMPORT_GRAMMAR, &[expression, &second], origin)
        }
        ExpansionError::Anchor { .. } | ExpansionError::Unreadable { .. } => {
            record(IMPORT_ANCHOR, &[&first, &second], origin)
        }
    }
}

/// The discovery grammars a reading's import expressions expand under:
/// a schema's compiled grammars when its data documents are read, or,
/// when definition documents are read, the grammars they declare,
/// gathered in reading order with the grammar a schema root binds.
#[derive(Debug, Default)]
pub(crate) struct DeclaredGrammars {
    pub(crate) names: Names,
    pub(crate) grammars: Vec<DiscoveryDecl>,
    root: Option<Sym>,
    /// A schema declaring profiles binds its grammar per invocation
    /// profile; a definition reading names none, so none is bound.
    profiled: bool,
}

impl DeclaredGrammars {
    /// The grammars a compiled definition declares, for reading its data
    /// documents; a definition declaring profiles binds none here.
    pub(crate) fn compiled(def: &Definition) -> Self {
        Self {
            names: def.names.clone(),
            grammars: def.discoveries.clone(),
            root: def.root_grammar,
            profiled: !def.profiles.is_empty(),
        }
    }

    /// Record what one document declares, read from its syntax, so a
    /// grammar written after the import it governs still governs it.
    /// Text that does not parse declares nothing; binding reports it.
    pub(crate) fn scan(&mut self, text: &str) {
        let Ok(parsed) = ParsedDocument::parse("", text) else {
            return;
        };
        let Some(root) = parsed.nodes().first().filter(|root| root.name == "schema") else {
            return;
        };
        if self.root.is_none()
            && let Some(grammar) = string_prop(root, "grammar")
        {
            self.root = Some(self.names.intern(grammar));
        }
        for child in &root.children {
            match child.name.as_str() {
                "discovery" => self.discovery(child),
                "profile" => self.profiled = true,
                _ => {}
            }
        }
    }

    /// The grammar the first schema root read binds, once a document
    /// read so far declares it; `None` admits literal paths only.
    pub(crate) fn grammar(&self) -> Option<&DiscoveryDecl> {
        if self.profiled {
            return None;
        }
        let root = self.root?;
        self.grammars.iter().find(|grammar| grammar.name == root)
    }

    /// One `discovery` declaration; the first of a name is kept, as the
    /// compiler reports any repeat.
    fn discovery(&mut self, node: &SyntaxNode) {
        let Some(name) = string_arg(node) else {
            return;
        };
        let name = self.names.intern(name);
        if self.grammars.iter().any(|grammar| grammar.name == name) {
            return;
        }
        let mut anchors: Vec<AnchorDecl> = Vec::new();
        for anchor in node.children.iter().filter(|c| c.name == "anchor") {
            let (Some(spelled), Some(provider)) =
                (string_arg(anchor), string_prop(anchor, "provider"))
            else {
                continue;
            };
            let sym = self.names.intern(spelled);
            if anchors.iter().any(|declared| declared.name == sym) {
                continue;
            }
            anchors.push(AnchorDecl {
                name: sym,
                provider: self.names.intern(provider),
                site: Site::new(0, anchor.span),
            });
        }
        let patterns = node
            .children
            .iter()
            .find(|c| c.name == "patterns")
            .map_or_else(Patterns::default, |rules| Patterns {
                star: flag(rules, "star"),
                doublestar: flag(rules, "doublestar"),
                braces: flag(rules, "braces"),
            });
        self.grammars.push(DiscoveryDecl {
            name,
            anchors,
            patterns,
            allow_empty: flag(node, "allow-empty"),
            site: Site::new(0, node.span),
        });
    }
}

fn string_arg(node: &SyntaxNode) -> Option<&str> {
    match node.args.first().map(|scalar| &scalar.value) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

fn string_prop<'n>(node: &'n SyntaxNode, key: &str) -> Option<&'n str> {
    match prop(node, key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

fn flag(node: &SyntaxNode, key: &str) -> bool {
    matches!(prop(node, key), Some(Value::Bool(true)))
}

/// The value of a property; the rightmost of a repeated key wins.
fn prop<'n>(node: &'n SyntaxNode, key: &str) -> Option<&'n Value> {
    node.props
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, scalar)| &scalar.value)
}
