//! A document as a tree that keeps what the event stream drops: comments,
//! where blank lines fell, slashdashed nodes, entries and children blocks,
//! and how each value was spelled. A formatter reads it; the tree accepts
//! exactly the documents [`crate::Parser`] accepts, with the same errors.

use kdlith_diagnostic::ByteSpan;

use crate::chars::is_newline;
use crate::error::{ErrorKind, Limit, ParseError, offset};
use crate::event::Value;
use crate::limits::Limits;
use crate::scan::{Result, Scanner};

/// A parsed document: its top-level block.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Document<'src> {
    pub block: Block<'src>,
}

/// The nodes and comments of a document or a children block, in order.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Block<'src> {
    pub items: Vec<Item<'src>>,
}

/// One line-level item of a block.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Item<'src> {
    /// At least one empty line separates this item from what precedes it
    /// in its block.
    pub blank_before: bool,
    /// The item starts on the line the preceding item, or the block's
    /// `{`, ended on.
    pub same_line: bool,
    pub kind: ItemKind<'src>,
}

/// A node, or a comment standing between nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind<'src> {
    Node(Node<'src>),
    Comment(Comment<'src>),
}

/// A `//` or `/* */` comment, spelled as written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Comment<'src> {
    /// The comment's text, its delimiters included and a line comment's
    /// newline excluded.
    pub text: &'src str,
    /// A `/* */` comment.
    pub block: bool,
    /// The column, in Unicode scalars from 0, the comment starts at.
    pub column: usize,
}

impl Comment<'_> {
    /// A line comment, which ends its line.
    pub fn is_line(&self) -> bool {
        !self.block
    }
}

/// A node: its header and its children blocks, slashdashed ones included.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Node<'src> {
    pub slashdash: bool,
    /// Comments written inside the node's head (after its `/-`, or in
    /// its type annotation), lifted to stand before it.
    pub leading: Vec<Comment<'src>>,
    pub ty: Option<Scalar<'src>>,
    pub name: Scalar<'src>,
    /// Entries, comments, and children blocks, in source order. No entry
    /// follows a children block.
    pub parts: Vec<Part<'src>>,
    pub span: ByteSpan,
}

impl<'src> Node<'src> {
    /// The node's entries, slashdashed ones included.
    pub fn entries(&self) -> impl Iterator<Item = &Entry<'src>> {
        self.parts.iter().filter_map(|part| match part {
            Part::Entry(entry) => Some(entry),
            _ => None,
        })
    }

    /// The node's real children block, if it has one.
    pub fn children(&self) -> Option<&Block<'src>> {
        self.parts.iter().find_map(|part| match part {
            Part::Children(children) if !children.slashdash => Some(&children.block),
            _ => None,
        })
    }
}

/// One element of a node after its name.
#[derive(Clone, Debug, PartialEq)]
pub enum Part<'src> {
    Entry(Entry<'src>),
    Comment(Comment<'src>),
    Children(Children<'src>),
}

/// An argument (no key) or a property.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Entry<'src> {
    pub slashdash: bool,
    pub key: Option<Scalar<'src>>,
    pub ty: Option<Scalar<'src>>,
    pub value: Scalar<'src>,
    pub span: ByteSpan,
}

/// A children block.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Children<'src> {
    pub slashdash: bool,
    pub block: Block<'src>,
}

/// A value with the way it was spelled.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Scalar<'src> {
    pub value: Value<'src>,
    pub spelling: Spelling,
}

impl Scalar<'_> {
    /// The string a string value holds.
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::String(text) => Some(text),
            _ => None,
        }
    }
}

/// How a value was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Spelling {
    /// An identifier string.
    Bare,
    /// `"..."`.
    Quoted,
    /// `#"..."#`.
    Raw,
    /// `"""` ... `"""`.
    MultiLine,
    /// `#"""` ... `"""#`.
    RawMultiLine,
    /// A number, bare or a `#` keyword number.
    Number,
    /// `#true`, `#false`, or `#null`.
    Keyword,
}

impl Spelling {
    /// Whether the value was written as a quoted, raw, or multi-line
    /// string: any string spelling but an identifier.
    pub fn is_quoted(self) -> bool {
        matches!(
            self,
            Spelling::Quoted | Spelling::Raw | Spelling::MultiLine | Spelling::RawMultiLine
        )
    }
}

impl<'src> Document<'src> {
    /// Parse `source` with the default [`Limits`].
    pub fn parse(source: &'src str) -> std::result::Result<Self, ParseError> {
        Self::parse_with_limits(source, Limits::DEFAULT)
    }

    /// Parse `source`; the first error ends the parse, as it ends
    /// [`crate::Parser`]'s stream.
    pub fn parse_with_limits(
        source: &'src str,
        limits: Limits,
    ) -> std::result::Result<Self, ParseError> {
        let max = limits.max_source_bytes;
        if source.len() > max {
            let kind = ErrorKind::LimitExceeded {
                limit: Limit::SourceBytes,
                max,
            };
            return Err(ParseError::new(kind, 0, max));
        }
        let mut builder = Builder {
            scan: Scanner::new(source, limits),
        };
        builder.scan.bom();
        let block = builder.block(None, 0)?;
        Ok(Document { block })
    }
}

/// What line space between items held.
struct Gap {
    /// Newlines since the previous item ended.
    newlines: usize,
}

struct Builder<'src> {
    scan: Scanner<'src>,
}

impl<'src> Builder<'src> {
    fn comment(&self, start: usize, block: bool) -> Comment<'src> {
        let source = self.scan.source();
        let mut end = self.scan.at();
        if !block {
            // A line comment's newline is not part of its text.
            while end > start {
                let Some(ch) = source[..end].chars().next_back() else {
                    break;
                };
                if !is_newline(ch) {
                    break;
                }
                end -= ch.len_utf8();
            }
        }
        let line = source[..start]
            .char_indices()
            .rev()
            .find(|(_, ch)| is_newline(*ch))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        Comment {
            text: &source[start..end],
            block,
            column: source[line..start].chars().count(),
        }
    }

    /// `escline` after its `\`, keeping its line comment.
    fn escline(&mut self, comments: &mut Vec<Comment<'src>>) -> Result<()> {
        loop {
            self.scan.node_space_plain()?;
            if self.scan.starts_with("/*") {
                let start = self.scan.at();
                self.scan.bump_byte();
                self.scan.bump_byte();
                self.scan.block_comment(start)?;
                comments.push(self.comment(start, true));
                continue;
            }
            break;
        }
        if self.scan.starts_with("//") {
            let start = self.scan.at();
            self.scan.bump_byte();
            self.scan.bump_byte();
            self.scan.line_comment()?;
            comments.push(self.comment(start, false));
            return Ok(());
        }
        if self.scan.peek().is_none() || self.scan.newline() {
            return Ok(());
        }
        Err(self.scan.error_here(ErrorKind::ExpectedNewline))
    }

    /// `node-space*`, keeping its comments; reports whether any space
    /// was consumed.
    fn node_space(&mut self, comments: &mut Vec<Comment<'src>>) -> Result<bool> {
        let start = self.scan.at();
        loop {
            self.scan.node_space_plain()?;
            match self.scan.peek() {
                Some('\\') => {
                    self.scan.bump_byte();
                    self.escline(comments)?;
                }
                Some('/') if self.scan.starts_with("/*") => {
                    let at = self.scan.at();
                    self.scan.bump_byte();
                    self.scan.bump_byte();
                    self.scan.block_comment(at)?;
                    comments.push(self.comment(at, true));
                }
                _ => return Ok(self.scan.at() != start),
            }
        }
    }

    /// `line-space*`, keeping its comments as items and counting its
    /// newlines into `gap`.
    fn line_space(&mut self, items: &mut Vec<Item<'src>>, gap: &mut Gap) -> Result<()> {
        loop {
            match self.scan.peek() {
                Some(ch) if is_newline(ch) => {
                    self.scan.newline();
                    gap.newlines += 1;
                }
                Some(ch) if crate::chars::is_space(ch) => self.scan.bump(ch),
                Some('\\') => {
                    self.scan.bump_byte();
                    let mut comments = Vec::new();
                    self.escline(&mut comments)?;
                    let had_line = comments.last().is_some_and(Comment::is_line);
                    for comment in comments {
                        self.push_comment(items, gap, comment);
                    }
                    // A continued line is no empty line; a line comment
                    // in it ends a line of its own.
                    if had_line {
                        gap.newlines = 1;
                    }
                }
                Some('/') if self.scan.starts_with("//") => {
                    let at = self.scan.at();
                    self.scan.bump_byte();
                    self.scan.bump_byte();
                    self.scan.line_comment()?;
                    let comment = self.comment(at, false);
                    self.push_comment(items, gap, comment);
                    gap.newlines = 1;
                }
                Some('/') if self.scan.starts_with("/*") => {
                    let at = self.scan.at();
                    self.scan.bump_byte();
                    self.scan.bump_byte();
                    self.scan.block_comment(at)?;
                    let comment = self.comment(at, true);
                    self.push_comment(items, gap, comment);
                }
                _ => return Ok(()),
            }
        }
    }

    fn push_comment(&self, items: &mut Vec<Item<'src>>, gap: &mut Gap, comment: Comment<'src>) {
        items.push(Item {
            blank_before: gap.newlines >= 2,
            same_line: gap.newlines == 0,
            kind: ItemKind::Comment(comment),
        });
        gap.newlines = 0;
    }

    /// `slashdash`, keeping the comments of the line space after it.
    fn slashdash(&mut self, comments: &mut Vec<Comment<'src>>) -> Result<bool> {
        if !self.scan.starts_with("/-") {
            return Ok(false);
        }
        self.scan.bump_byte();
        self.scan.bump_byte();
        let mut items = Vec::new();
        let mut gap = Gap { newlines: 0 };
        self.line_space(&mut items, &mut gap)?;
        comments.extend(items.into_iter().filter_map(|item| match item.kind {
            ItemKind::Comment(comment) => Some(comment),
            ItemKind::Node(_) => None,
        }));
        Ok(true)
    }

    /// The nodes of a block up to its `}` (`open` names where its `{`
    /// was), or up to the end of input at the top level.
    fn block(&mut self, open: Option<usize>, depth: usize) -> Result<Block<'src>> {
        let mut items = Vec::new();
        // At the top level nothing precedes the first item on its line.
        let mut gap = Gap {
            newlines: usize::from(open.is_none()),
        };
        loop {
            self.line_space(&mut items, &mut gap)?;
            let start = self.scan.at();
            match self.scan.peek() {
                None => {
                    if let Some(open) = open {
                        let kind = ErrorKind::Unterminated(crate::error::Construct::Children);
                        return Err(ParseError::new(kind, open, start));
                    }
                    return Ok(Block { items });
                }
                Some('}') => {
                    if open.is_none() {
                        return Err(self.scan.error_here(ErrorKind::UnmatchedClose));
                    }
                    self.scan.bump_byte();
                    return Ok(Block { items });
                }
                Some(_) => {
                    let blank_before = gap.newlines >= 2;
                    let same_line = gap.newlines == 0;
                    let (node, newlines) = self.node(depth)?;
                    items.push(Item {
                        blank_before,
                        same_line,
                        kind: ItemKind::Node(node),
                    });
                    gap.newlines = newlines;
                }
            }
        }
    }

    /// A type annotation, its inner comments kept.
    fn annotation(&mut self, comments: &mut Vec<Comment<'src>>) -> Result<Option<Scalar<'src>>> {
        if self.scan.peek() != Some('(') {
            return Ok(None);
        }
        self.scan.bump_byte();
        self.node_space(comments)?;
        let start = self.scan.at();
        let ty = self.scalar()?;
        if !matches!(ty.value, Value::String(_)) {
            return Err(ParseError::new(
                ErrorKind::ExpectedString,
                start,
                self.scan.at(),
            ));
        }
        self.node_space(comments)?;
        if self.scan.peek() != Some(')') {
            return Err(self.scan.error_here(ErrorKind::ExpectedCloseParen));
        }
        self.scan.bump_byte();
        Ok(Some(ty))
    }

    /// A value and its spelling.
    fn scalar(&mut self) -> Result<Scalar<'src>> {
        let start = self.scan.at();
        let value = self.scan.value()?;
        let written = &self.scan.source()[start..self.scan.at()];
        let spelling = match &value {
            Value::Number(_) => Spelling::Number,
            Value::Bool(_) | Value::Null => Spelling::Keyword,
            Value::String(_) => {
                let hashes = written.len() - written.trim_start_matches('#').len();
                let body = &written[hashes..];
                match (hashes > 0, body.starts_with("\"\"\"")) {
                    (_, _) if !body.starts_with('"') => Spelling::Bare,
                    (false, false) => Spelling::Quoted,
                    (false, true) => Spelling::MultiLine,
                    (true, false) => Spelling::Raw,
                    (true, true) => Spelling::RawMultiLine,
                }
            }
        };
        Ok(Scalar { value, spelling })
    }

    /// A string: a node name or a property key.
    fn string(&mut self) -> Result<Scalar<'src>> {
        let start = self.scan.at();
        let scalar = self.scalar()?;
        match scalar.value {
            Value::String(_) => Ok(scalar),
            _ => Err(ParseError::new(
                ErrorKind::ExpectedString,
                start,
                self.scan.at(),
            )),
        }
    }

    /// One node, through its terminator; also how many newlines the
    /// terminator consumed.
    fn node(&mut self, depth: usize) -> Result<(Node<'src>, usize)> {
        let start = self.scan.at();
        let mut leading = Vec::new();
        let slashdash = self.slashdash(&mut leading)?;
        let ty = self.annotation(&mut leading)?;
        self.node_space(&mut leading)?;
        let name = self.string()?;
        let mut parts: Vec<Part<'src>> = Vec::new();
        let mut entries = true;
        let mut ended = false;
        let mut spaced = false;
        let newlines;
        loop {
            let mut comments = Vec::new();
            spaced = self.node_space(&mut comments)? || spaced;
            parts.extend(comments.into_iter().map(Part::Comment));
            match self.scan.peek() {
                None | Some('}') => {
                    newlines = 0;
                    break;
                }
                Some(';') => {
                    self.scan.bump_byte();
                    newlines = 0;
                    break;
                }
                Some('/') if self.scan.starts_with("//") => {
                    let at = self.scan.at();
                    self.scan.bump_byte();
                    self.scan.bump_byte();
                    self.scan.line_comment()?;
                    parts.push(Part::Comment(self.comment(at, false)));
                    newlines = 1;
                    break;
                }
                Some(_) if self.scan.newline() => {
                    newlines = 1;
                    break;
                }
                Some(_) => {}
            }
            let mut before = Vec::new();
            let entry_slashdash = self.slashdash(&mut before)?;
            parts.extend(before.into_iter().map(Part::Comment));
            let at = self.scan.at();
            if self.scan.peek() == Some('{') {
                if !entry_slashdash && ended {
                    return Err(self.scan.error_here(ErrorKind::MultipleChildren));
                }
                let max = self.scan.limits().max_depth;
                if depth >= max {
                    let kind = ErrorKind::LimitExceeded {
                        limit: Limit::Depth,
                        max,
                    };
                    return Err(self.scan.error_here(kind));
                }
                self.scan.bump_byte();
                let block = self.block(Some(at), depth + 1)?;
                parts.push(Part::Children(Children {
                    slashdash: entry_slashdash,
                    block,
                }));
                entries = false;
                ended |= !entry_slashdash;
                spaced = false;
                continue;
            }
            if !entries {
                return Err(self.scan.error_here(ErrorKind::EntryAfterChildren));
            }
            if !entry_slashdash && !spaced {
                return Err(self.scan.error_here(ErrorKind::ExpectedSpace));
            }
            spaced = self.entry(entry_slashdash, at, &mut parts)?;
        }
        let end = self.scan.at();
        let node = Node {
            slashdash,
            leading,
            ty,
            name,
            parts,
            span: ByteSpan::new(offset(start), offset(end)),
        };
        Ok((node, newlines))
    }

    /// `node-prop-or-arg`, pushed with the comments written inside it;
    /// reports whether node space followed an argument.
    fn entry(
        &mut self,
        slashdash: bool,
        start: usize,
        parts: &mut Vec<Part<'src>>,
    ) -> Result<bool> {
        let mut inner = Vec::new();
        let span = |builder: &Self| ByteSpan::new(offset(start), offset(builder.scan.at()));
        if let Some(ty) = self.annotation(&mut inner)? {
            self.node_space(&mut inner)?;
            let value = self.scalar()?;
            parts.extend(inner.into_iter().map(Part::Comment));
            parts.push(Part::Entry(Entry {
                slashdash,
                key: None,
                ty: Some(ty),
                value,
                span: span(self),
            }));
            return Ok(false);
        }
        let value = self.scalar()?;
        let end = self.scan.at();
        let mut after = Vec::new();
        let spaced = self.node_space(&mut after)?;
        if !matches!(value.value, Value::String(_)) || self.scan.peek() != Some('=') {
            parts.push(Part::Entry(Entry {
                slashdash,
                key: None,
                ty: None,
                value,
                span: ByteSpan::new(offset(start), offset(end)),
            }));
            parts.extend(after.into_iter().map(Part::Comment));
            return Ok(spaced);
        }
        self.scan.bump_byte();
        inner.extend(after);
        self.node_space(&mut inner)?;
        let ty = self.annotation(&mut inner)?;
        self.node_space(&mut inner)?;
        let key = value;
        let value = self.scalar()?;
        parts.extend(inner.into_iter().map(Part::Comment));
        parts.push(Part::Entry(Entry {
            slashdash,
            key: Some(key),
            ty,
            value,
            span: span(self),
        }));
        Ok(false)
    }
}
