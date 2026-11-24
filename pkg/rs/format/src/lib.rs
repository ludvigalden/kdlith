//! The canonical KDL layout. [`format`] reads a document into a tree that
//! keeps its comments and slashdashed text, rewrites it as a schema's
//! [`Policy`] says (when one is given), and writes it in one canonical
//! layout: the output depends on what the document means, its comments,
//! where its blank lines fell, and which strings were written bare, never
//! on how it was otherwise spaced, wrapped, quoted, or escaped.
//!
//! Formatting never changes a value: a long string is wrapped only
//! through whitespace escapes, which read back as nothing. Every result
//! is read back and compared with the tree it was written from before it
//! is returned, so a layout fault fails closed as [`FormatError::Unstable`]
//! and never as a changed document.
mod policy;
mod render;
mod strings;
mod verify;

use kdlith_parse::tree::Document;
use kdlith_parse::{Limits as ParseLimits, ParseError};

pub use policy::{MIN_WIDTH, NodeRule, Order, Policy, Text, ValueRule, fold_prose};

/// The default line width: no line runs past this column.
pub const DEFAULT_WIDTH: usize = 100;
/// The default content width: no line holds more than this many
/// columns after its indentation.
pub const DEFAULT_CONTENT_WIDTH: usize = 80;
/// Spaces per nesting level.
pub const INDENT: usize = 4;

/// The two line limits. A line at indentation `i` ends at or before
/// column `min(width, i + content_width)`, wherever a break can fall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub width: usize,
    pub content_width: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            content_width: DEFAULT_CONTENT_WIDTH,
        }
    }
}

/// How to format: the line limits a policy does not override, and the
/// spaces per level.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    pub limits: Limits,
    pub indent: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            indent: INDENT,
        }
    }
}

impl Options {
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
}

/// Why a document was not formatted.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatError {
    /// The document is not KDL v2.
    Parse(ParseError),
    /// The layout written would not read back as the document: a fault in
    /// the formatter, reported instead of returned.
    Unstable,
}

/// Format `source`. The result ends in a newline unless the document is
/// empty.
pub fn format(
    source: &str,
    options: &Options,
    policy: Option<&Policy>,
) -> Result<String, FormatError> {
    let mut document =
        Document::parse_with_limits(source, ParseLimits::DEFAULT).map_err(FormatError::Parse)?;
    let before = policy.map(|_| verify::tokens(&document.block));
    let marks = policy
        .map(|policy| policy.apply(&mut document.block))
        .unwrap_or_default();
    let mut renderer = render::Renderer::new(options, policy, &marks, source.len());
    renderer.document(&document.block);
    let out = renderer.out;
    let expected = verify::tokens(&document.block);
    // The text written, read back under the same rules (which fold its
    // prose as a reading does), holds the document the rules made.
    let mut written = Document::parse(&out).map_err(|_| FormatError::Unstable)?;
    if let Some(policy) = policy {
        policy.apply(&mut written.block);
    }
    if !verify::same(&expected, &verify::tokens(&written.block)) {
        return Err(FormatError::Unstable);
    }
    // When the rules rewrote nothing, the event parser, which shares no
    // code with the tree above it, must read the same stream from both
    // texts.
    let rewritten = before.is_some_and(|before| before != expected)
        || marks.values().any(|text| *text == Text::Prose);
    if !rewritten && !verify::same_events(source, &out) {
        return Err(FormatError::Unstable);
    }
    Ok(out)
}
