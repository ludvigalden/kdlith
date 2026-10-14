//! Discovery grammars: the restricted path-expansion grammar import
//! expressions are evaluated under. Anchors are host-supplied typed
//! paths; no grammar admits env interpolation. Matches order by
//! portable path bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::definition::{DiscoveryDecl, Patterns};
use crate::names::Names;
use crate::names::Sym;

/// Whether an import path uses discovery syntax rather than a literal
/// path: anchors, globs, or brace alternatives.
pub fn is_expression(path: &str) -> bool {
    path.starts_with('$') || path.contains(['*', '{', '}'])
}

/// One segment of a parsed import expression.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Segment {
    /// A grammar-declared anchor, host-supplied as a typed path.
    Anchor(Sym),
    /// A pattern segment: `star` and `braces` within one path segment,
    /// `doublestar` spanning segments.
    Pattern {
        doublestar: bool,
        star: bool,
        braces: bool,
        text: String,
    },
    /// A literal path segment.
    Literal(String),
}

/// A parsed import expression: the original spelling and its segments.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Expansion {
    pub expression: String,
    pub segments: Vec<Segment>,
}

/// Why an import expression does not evaluate under its grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExpansionError {
    /// The expression's syntax is not admitted: an unadmitted pattern
    /// kind, environment-variable interpolation (which no grammar
    /// admits), or a malformed expression (`KDLITH-IMP004`).
    NotAdmitted { reason: String },
    /// The expression names an anchor the grammar does not declare, or
    /// declares but the host does not supply (`KDLITH-IMP005`).
    Anchor { name: String },
    /// A directory the expansion reads cannot be read.
    Unreadable { path: String, reason: String },
}

impl ExpansionError {
    /// The registered import code this error reports under.
    pub fn code(&self) -> &'static str {
        match self {
            ExpansionError::NotAdmitted { .. } => "KDLITH-IMP004",
            ExpansionError::Anchor { .. } | ExpansionError::Unreadable { .. } => "KDLITH-IMP005",
        }
    }

    /// The values the code's message renders.
    pub fn values(&self) -> [String; 2] {
        match self {
            ExpansionError::NotAdmitted { reason } => {
                [reason.clone(), "the active grammar".to_owned()]
            }
            ExpansionError::Anchor { name } => [name.clone(), "the active grammar".to_owned()],
            ExpansionError::Unreadable { path, reason } => [path.clone(), reason.clone()],
        }
    }
}

/// Parse an import expression under `grammar`, validating every anchor
/// and pattern kind it uses. `None` admits literal paths only.
pub fn parse(
    expression: &str,
    grammar: Option<&DiscoveryDecl>,
    names: &Names,
) -> Result<Expansion, ExpansionError> {
    let not_admitted = |reason: &str| ExpansionError::NotAdmitted {
        reason: format!("import {expression:?} {reason}"),
    };
    if expression.is_empty() {
        return Err(not_admitted("is empty"));
    }
    let mut segments = Vec::new();
    for part in expression.split('/') {
        if part.is_empty() {
            return Err(not_admitted("has an empty path segment"));
        }
        segments.push(segment(part, grammar, names, &not_admitted)?);
    }
    Ok(Expansion {
        expression: expression.to_owned(),
        segments,
    })
}

fn segment(
    part: &str,
    grammar: Option<&DiscoveryDecl>,
    names: &Names,
    not_admitted: &dyn Fn(&str) -> ExpansionError,
) -> Result<Segment, ExpansionError> {
    if let Some(name) = part.strip_prefix('$') {
        // Environment-variable interpolation is admitted by no grammar;
        // an anchor is a whole segment naming a grammar-declared anchor.
        if name.starts_with('{')
            || name.contains(['$', '/', '{', '}'])
            || name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(not_admitted(
                "uses environment-variable interpolation or a malformed anchor",
            ));
        }
        let declared = grammar.is_some_and(|g| {
            g.anchors
                .iter()
                .any(|anchor| names.resolve(anchor.name) == name)
        });
        if !declared {
            return Err(ExpansionError::Anchor {
                name: name.to_owned(),
            });
        }
        let sym = grammar
            .and_then(|g| g.anchors.iter().find(|a| names.resolve(a.name) == name))
            .map(|anchor| anchor.name)
            .expect("the anchor was found declared");
        return Ok(Segment::Anchor(sym));
    }
    if part.contains('$') {
        return Err(not_admitted(
            "uses environment-variable interpolation or a malformed anchor",
        ));
    }
    if part.contains('?') || part.contains('[') {
        return Err(not_admitted(
            "uses pattern syntax no grammar defines (only star, doublestar, and braces)",
        ));
    }
    let doublestar = part.contains("**");
    let braces = part.contains('{') || part.contains('}');
    let star = !doublestar && part.contains('*');
    if doublestar && !grammar.is_some_and(|g| g.patterns.doublestar) {
        return Err(not_admitted(
            "uses doublestar, which the grammar does not admit",
        ));
    }
    if star && !grammar.is_some_and(|g| g.patterns.star) {
        return Err(not_admitted("uses star, which the grammar does not admit"));
    }
    if braces {
        if !grammar.is_some_and(|g| g.patterns.braces) {
            return Err(not_admitted(
                "uses brace alternatives, which the grammar does not admit",
            ));
        }
        if !balanced_alternatives(part) {
            return Err(not_admitted("has malformed brace alternatives"));
        }
    }
    if doublestar || star || braces {
        return Ok(Segment::Pattern {
            doublestar,
            star,
            braces,
            text: part.to_owned(),
        });
    }
    Ok(Segment::Literal(part.to_owned()))
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

/// Expand a parsed expression against the host-supplied anchor values,
/// rooted at `base` for expressions that start without an anchor.
/// Matches are ordered by their portable path bytes and deduplicated.
pub fn expand(
    expansion: &Expansion,
    names: &Names,
    anchors: &BTreeMap<Sym, PathBuf>,
    base: &Path,
) -> Result<Vec<PathBuf>, ExpansionError> {
    let mut roots = vec![base.to_owned()];
    let mut at = 0;
    if let Some(Segment::Anchor(name)) = expansion.segments.first() {
        let value = anchors.get(name).ok_or_else(|| ExpansionError::Anchor {
            name: names.resolve(*name).to_owned(),
        })?;
        roots = vec![value.clone()];
        at = 1;
    }
    let mut matches = Vec::new();
    for root in &roots {
        walk(&mut matches, names, root, &expansion.segments[at..])?;
    }
    matches.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
    matches.dedup();
    Ok(matches)
}

/// Collect every path below `dir` the remaining segments admit.
fn walk(
    matches: &mut Vec<PathBuf>,
    names: &Names,
    dir: &Path,
    rest: &[Segment],
) -> Result<(), ExpansionError> {
    let Some(segment) = rest.first() else {
        matches.push(dir.to_owned());
        return Ok(());
    };
    let further = &rest[1..];
    match segment {
        Segment::Literal(text) => walk(matches, names, &dir.join(text), further),
        Segment::Anchor(name) => Err(ExpansionError::Anchor {
            name: names.resolve(*name).to_owned(),
        }),
        Segment::Pattern {
            doublestar, text, ..
        } => {
            let entries = read_dir(dir)?;
            if *doublestar {
                // A doublestar segment matches this directory and every
                // directory below it, then the rest continues.
                walk(matches, names, dir, further)?;
                for child in &entries {
                    if child.is_dir() {
                        walk(matches, names, child, rest)?;
                    }
                }
                return Ok(());
            }
            for child in &entries {
                let name = match child.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_owned(),
                    None => continue,
                };
                if !matches_pattern(text, &name) {
                    continue;
                }
                if further.is_empty() && !child.is_file() {
                    continue;
                }
                walk(matches, names, child, further)?;
            }
            Ok(())
        }
    }
}

/// The candidate entries of one directory, sorted by name bytes; a
/// directory that cannot be read fails closed.
fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, ExpansionError> {
    let mut entries = Vec::new();
    let read = std::fs::read_dir(dir).map_err(|error| ExpansionError::Unreadable {
        path: dir.display().to_string(),
        reason: error.to_string(),
    })?;
    for entry in read.flatten() {
        entries.push(entry.path());
    }
    entries.sort_by(|a, b| {
        a.file_name()
            .map(|n| n.as_encoded_bytes().to_owned())
            .unwrap_or_default()
            .cmp(
                &b.file_name()
                    .map(|n| n.as_encoded_bytes().to_owned())
                    .unwrap_or_default(),
            )
    });
    Ok(entries)
}

/// Whether one path segment matches a pattern segment: `*` within the
/// segment and flat `{a,b}` alternatives.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    for alternative in alternatives(pattern) {
        if matches_star(&alternative, name) {
            return true;
        }
    }
    false
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

/// `*` matches any run of bytes except `/` (already excluded: one
/// segment at a time), including none.
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
            // Try every split point for the segment after `*`.
            let bytes = name.as_bytes();
            (0..=bytes.len())
                .filter(|at| name.is_char_boundary(*at))
                .any(|at| matches_star(rest, &name[at..]))
        }
    }
}

/// The patterns of a grammar as its declaration spells them, for
/// summaries and diagnostics.
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
