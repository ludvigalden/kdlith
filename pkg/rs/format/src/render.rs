//! The canonical layout: one node per line at four spaces per level,
//! single spaces between entries, a header that outgrows its line
//! filled onto continuation lines, a string that cannot fit written as
//! a multi-line string, comments kept where they were, and at most one
//! empty line between items.

use kdlith_parse::Value;
use kdlith_parse::tree::{Block, Comment, Entry, ItemKind, Node, Part, Scalar, Spelling};

use crate::policy::{Marks, Policy, Text};
use crate::strings::{self, MultiLine, pad, width};
use crate::{Limits, Options};

pub(crate) struct Renderer<'a> {
    pub(crate) out: String,
    options: &'a Options,
    policy: Option<&'a Policy>,
    marks: &'a Marks,
}

/// The line limits in effect at one node.
#[derive(Clone, Copy)]
struct Scope {
    limits: Limits,
}

/// A string that becomes a multi-line string when it cannot fit.
struct Prose {
    prefix: String,
    value: String,
    /// Refilled as prose rather than wrapped through escapes.
    reflow: bool,
}

impl Prose {
    fn multi(&self) -> MultiLine {
        if self.reflow {
            strings::prose(&self.value)
        } else {
            strings::multi_line(&self.value)
        }
    }
}

/// One element of a node header as laid out.
enum Atom {
    /// Single-line text; `prose` when it is a string that may become a
    /// multi-line string, with that string's prefix and value.
    Text { text: String, prose: Option<Prose> },
    /// A multi-line string after its prefix (`key=`, a type annotation).
    Multi { prefix: String, string: MultiLine },
    /// A `/* */` comment spanning lines, with the column it was written at.
    Verbatim { comment: String, column: usize },
    /// A `//` comment: it ends its line.
    LineComment(String),
}

impl<'a> Renderer<'a> {
    pub(crate) fn new(
        options: &'a Options,
        policy: Option<&'a Policy>,
        marks: &'a Marks,
        capacity: usize,
    ) -> Self {
        Self {
            out: String::with_capacity(capacity + capacity / 8),
            options,
            policy,
            marks,
        }
    }

    pub(crate) fn document(&mut self, block: &Block<'_>) {
        let limits = self.policy.map_or(self.options.limits, |policy| {
            policy.root_limits(self.options.limits)
        });
        let scope = Scope { limits };
        self.block(block, 0, scope, None);
    }

    fn unit(&self) -> usize {
        self.options.indent
    }

    /// The column after which no text should run on a line at `indent`.
    fn limit(limits: Limits, indent: usize) -> usize {
        limits.width.min(indent + limits.content_width)
    }

    /// The column the output's current line has reached.
    fn column(&self) -> usize {
        let line = self.out.rfind('\n').map_or(0, |at| at + 1);
        width(&self.out[line..])
    }

    /// Append to the line just written, before its newline.
    fn reopen_line(&mut self) -> bool {
        if self.out.ends_with('\n') {
            self.out.pop();
            true
        } else {
            false
        }
    }

    fn block(&mut self, block: &Block<'_>, depth: usize, scope: Scope, parent: Option<&str>) {
        let indent = depth * self.unit();
        let mut first = true;
        let mut previous_inline_comment = false;
        for item in &block.items {
            let inline_comment = matches!(&item.kind,
                ItemKind::Comment(comment) if !comment.is_line() && !comment.text.contains('\n'));
            match &item.kind {
                ItemKind::Comment(comment) if item.same_line && self.reopen_line() => {
                    self.out.push(' ');
                    self.comment(comment);
                    self.out.push('\n');
                }
                ItemKind::Comment(comment) => {
                    if item.blank_before && !first {
                        self.out.push('\n');
                    }
                    pad(&mut self.out, indent);
                    self.comment(comment);
                    self.out.push('\n');
                }
                ItemKind::Node(node) => {
                    let scope = self.enter(scope, parent, node);
                    // A node written after an inline `/* */` on its line
                    // stays on that line.
                    let inline =
                        item.same_line && !first && previous_inline_comment && self.reopen_line();
                    if inline {
                        self.out.push(' ');
                    } else if item.blank_before && !first {
                        self.out.push('\n');
                    }
                    self.node(node, depth, scope, !inline);
                }
            }
            first = false;
            previous_inline_comment = inline_comment;
        }
    }

    fn enter(&self, scope: Scope, parent: Option<&str>, node: &Node<'_>) -> Scope {
        let Some(policy) = self.policy else {
            return scope;
        };
        let name = node.name.as_str().unwrap_or_default();
        let limits = policy
            .resolve(parent, name)
            .map_or(scope.limits, |rule| policy.limits(rule, scope.limits));
        Scope { limits }
    }

    /// Write a comment at the current column: a line comment without its
    /// trailing whitespace, a block comment with its inner lines moved
    /// as far as its first line moved.
    fn comment(&mut self, comment: &Comment<'_>) {
        if comment.is_line() {
            self.out.push_str(comment.text.trim_end());
            return;
        }
        let column = self.column();
        self.verbatim(comment.text, comment.column, column);
    }

    fn verbatim(&mut self, text: &str, from: usize, to: usize) {
        let mut lines = text.split('\n');
        if let Some(first) = lines.next() {
            self.out.push_str(first.trim_end_matches([' ', '\t', '\r']));
        }
        let rest: Vec<&str> = lines.collect();
        let shift = |line: &str| line.len() - line.trim_start_matches(' ').len();
        let removable = rest
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| shift(line))
            .min()
            .unwrap_or(0);
        for line in rest {
            self.out.push('\n');
            let line = line.trim_end_matches([' ', '\t', '\r']);
            if line.is_empty() {
                continue;
            }
            if to >= from {
                pad(&mut self.out, to - from);
                self.out.push_str(line);
            } else {
                let cut = (from - to).min(removable);
                self.out.push_str(&line[cut..]);
            }
        }
    }

    fn node(&mut self, node: &Node<'_>, depth: usize, scope: Scope, at_line_start: bool) {
        let indent = depth * self.unit();
        let mut at_line_start = at_line_start;
        for comment in &node.leading {
            if at_line_start {
                pad(&mut self.out, indent);
            }
            self.comment(comment);
            // A `/* */` on one line stays beside the node.
            if comment.is_line() || comment.text.contains('\n') {
                self.out.push('\n');
                at_line_start = true;
            } else {
                self.out.push(' ');
                at_line_start = false;
            }
        }
        if at_line_start {
            pad(&mut self.out, indent);
        }
        let mut head = String::new();
        if node.slashdash {
            head.push_str("/-");
        }
        if let Some(ty) = &node.ty {
            push_annotation(&mut head, ty);
        }
        head.push_str(&strings::name(node.name.as_str().unwrap_or_default()));
        self.out.push_str(&head);

        let split = node
            .parts
            .iter()
            .position(|part| matches!(part, Part::Children(_)))
            .unwrap_or(node.parts.len());
        let (header, tail) = node.parts.split_at(split);
        // Line comments written after the last entry and before the first
        // block move to the block's `{` line; a `//` cannot precede `{`.
        let last_entry = header
            .iter()
            .rposition(|part| matches!(part, Part::Entry(_)))
            .map_or(0, |at| at + 1);
        let moves = |at: usize, part: &Part<'_>| {
            !tail.is_empty()
                && at >= last_entry
                && matches!(part, Part::Comment(comment) if comment.is_line())
        };
        let moved: Vec<&Comment<'_>> = header
            .iter()
            .enumerate()
            .filter_map(|(at, part)| match part {
                Part::Comment(comment) if moves(at, part) => Some(comment),
                _ => None,
            })
            .collect();
        let atoms: Vec<Atom> = header
            .iter()
            .enumerate()
            .filter(|(at, part)| !moves(*at, part))
            .filter_map(|(_, part)| match part {
                Part::Entry(entry) => Some(self.entry_atom(entry)),
                Part::Comment(comment) if comment.is_line() => {
                    Some(Atom::LineComment(comment.text.trim_end().to_owned()))
                }
                Part::Comment(comment) if comment.text.contains('\n') => Some(Atom::Verbatim {
                    comment: comment.text.to_owned(),
                    column: comment.column,
                }),
                Part::Comment(comment) => Some(Atom::Text {
                    text: comment.text.to_owned(),
                    prose: None,
                }),
                Part::Children(_) => None,
            })
            .collect();
        let opens = !tail.is_empty();
        self.header(atoms, indent, scope.limits, opens);

        let mut first_block = true;
        for (index, part) in tail.iter().enumerate() {
            match part {
                Part::Children(children) => {
                    self.out.push(' ');
                    if children.slashdash {
                        self.out.push_str("/-");
                    }
                    self.out.push('{');
                    if first_block {
                        for comment in &moved {
                            self.out.push(' ');
                            self.comment(comment);
                        }
                    }
                    let empty =
                        children.block.items.is_empty() && (!first_block || moved.is_empty());
                    first_block = false;
                    if empty {
                        self.out.push('}');
                        continue;
                    }
                    self.out.push('\n');
                    let parent = node.name.as_str();
                    self.block(&children.block, depth + 1, scope, parent);
                    pad(&mut self.out, indent);
                    self.out.push('}');
                }
                Part::Comment(comment) => {
                    let more = tail[index + 1..]
                        .iter()
                        .any(|part| matches!(part, Part::Children(_)));
                    self.out.push(' ');
                    if comment.is_line() && more {
                        self.out.push_str("\\ ");
                        self.comment(comment);
                        self.out.push('\n');
                        let continuation = indent + 2 * self.unit();
                        pad(&mut self.out, continuation);
                        // The next block's ` {` supplies its own space.
                        self.out.pop();
                    } else {
                        self.comment(comment);
                    }
                }
                Part::Entry(_) => {}
            }
        }
        self.out.push('\n');
    }

    /// Lay out the header's atoms after the head already written: on one
    /// line when they fit, else filled onto continuation lines two levels
    /// in, with strings too long for any line written as multi-line
    /// strings.
    fn header(&mut self, mut atoms: Vec<Atom>, indent: usize, limits: Limits, opens: bool) {
        let continuation = indent + 2 * self.unit();
        let opener = if opens { 2 } else { 0 };
        // The last atom that is not a trailing line comment must leave
        // room for the block's ` {`.
        let last_code = atoms
            .iter()
            .rposition(|atom| !matches!(atom, Atom::LineComment(_)));
        let one_line = atoms
            .iter()
            .all(|atom| matches!(atom, Atom::Text { .. } | Atom::LineComment(_)))
            && {
                let total: usize = atoms
                    .iter()
                    .map(|atom| match atom {
                        Atom::Text { text, .. } => 1 + width(text),
                        _ => 0,
                    })
                    .sum();
                self.column() + total + opener <= Self::limit(limits, indent)
            }
            && atoms
                .iter()
                .enumerate()
                .all(|(at, atom)| !matches!(atom, Atom::LineComment(_)) || Some(at) > last_code);
        if one_line {
            for atom in atoms {
                match atom {
                    Atom::Text { text, .. } => {
                        self.out.push(' ');
                        self.out.push_str(&text);
                    }
                    Atom::LineComment(text) => {
                        self.out.push(' ');
                        self.out.push_str(&text);
                    }
                    _ => {}
                }
            }
            return;
        }
        let mut line_indent = indent;
        // Whether the current line holds anything after the head or the
        // continuation's indentation.
        let mut filled = false;
        let count = atoms.len();
        for (at, atom) in atoms.drain(..).enumerate() {
            let reserve = if Some(at) == last_code { opener } else { 0 };
            let limit = Self::limit(limits, line_indent);
            match atom {
                Atom::LineComment(text) => {
                    self.out.push(' ');
                    let more = at + 1 < count;
                    if more {
                        self.out.push_str("\\ ");
                        self.out.push_str(&text);
                        self.out.push('\n');
                        pad(&mut self.out, continuation);
                        line_indent = continuation;
                        filled = false;
                        self.out.pop();
                        // Every atom after pushes its own leading space.
                    } else {
                        self.out.push_str(&text);
                    }
                }
                Atom::Text { text, prose } => {
                    let column = self.column();
                    let fits = column + 1 + width(&text) + reserve <= limit;
                    if fits {
                        self.out.push(' ');
                        self.out.push_str(&text);
                        filled = true;
                        continue;
                    }
                    let cont_limit = Self::limit(limits, continuation);
                    let fits_alone = continuation + width(&text) + reserve <= cont_limit;
                    if let Some(prose) = prose
                        && (!filled || !fits_alone)
                    {
                        let string = prose.multi();
                        let prefix = prose.prefix;
                        let column = self.column();
                        if filled && column + 1 + width(&prefix) + 3 > limit {
                            self.break_line(continuation);
                            line_indent = continuation;
                        } else {
                            self.out.push(' ');
                        }
                        line_indent = self.multi(&prefix, &string, line_indent, limits);
                        filled = true;
                        continue;
                    }
                    if filled {
                        self.break_line(continuation);
                        line_indent = continuation;
                    } else {
                        self.out.push(' ');
                    }
                    self.out.push_str(&text);
                    filled = true;
                }
                Atom::Multi { prefix, string } => {
                    let column = self.column();
                    if filled && column + 1 + width(&prefix) + 3 > limit {
                        self.break_line(continuation);
                        line_indent = continuation;
                    } else {
                        self.out.push(' ');
                    }
                    line_indent = self.multi(&prefix, &string, line_indent, limits);
                    filled = true;
                }
                Atom::Verbatim { comment, column } => {
                    self.out.push(' ');
                    let to = self.column();
                    self.verbatim(&comment, column, to);
                    filled = true;
                }
            }
        }
    }

    /// End the current line with a continuation and start the next at
    /// `indent`, ready for text with no leading space.
    fn break_line(&mut self, indent: usize) {
        self.out.push_str(" \\\n");
        pad(&mut self.out, indent);
    }

    /// Write a multi-line string opening on the current line, whose
    /// logical indentation is `line_indent`; text after its closing
    /// delimiter continues that line, so the indentation is returned.
    fn multi(
        &mut self,
        prefix: &str,
        string: &MultiLine,
        line_indent: usize,
        limits: Limits,
    ) -> usize {
        self.out.push_str(prefix);
        self.out.push_str(&string.opener());
        self.out.push('\n');
        let body = line_indent + self.unit();
        string.render(&mut self.out, body, Self::limit(limits, body));
        pad(&mut self.out, body);
        self.out.push_str(&string.closer());
        line_indent
    }

    fn entry_atom(&self, entry: &Entry<'_>) -> Atom {
        let mut prefix = String::new();
        if entry.slashdash {
            prefix.push_str("/-");
        }
        if let Some(key) = &entry.key {
            prefix.push_str(&strings::name(key.as_str().unwrap_or_default()));
            prefix.push('=');
        }
        if let Some(ty) = &entry.ty {
            push_annotation(&mut prefix, ty);
        }
        let kind = self
            .marks
            .get(&(entry.span.start, entry.span.end))
            .copied()
            .unwrap_or_default();
        match &entry.value.value {
            Value::String(value) if entry.value.spelling != Spelling::Bare => {
                if value.contains('\n') {
                    let string = match kind {
                        Text::Prose => strings::prose(value),
                        Text::Verbatim => strings::verbatim(value),
                        Text::Exact => strings::multi_line(value),
                    };
                    return Atom::Multi { prefix, string };
                }
                let text = format!("{prefix}{}", strings::single_line(value));
                let prose = match kind {
                    Text::Exact => strings::wrappable(value),
                    Text::Prose => strings::breakable(value),
                    Text::Verbatim => false,
                }
                .then(|| Prose {
                    prefix,
                    value: value.to_string(),
                    reflow: kind == Text::Prose,
                });
                Atom::Text { text, prose }
            }
            _ => {
                let mut text = prefix;
                push_scalar(&mut text, &entry.value);
                Atom::Text { text, prose: None }
            }
        }
    }
}

fn push_annotation(out: &mut String, ty: &Scalar<'_>) {
    out.push('(');
    out.push_str(&strings::name(ty.as_str().unwrap_or_default()));
    out.push(')');
}

/// A value on one line, as its spelling class says: a bare string bare,
/// a number as spelled, a keyword as its keyword.
fn push_scalar(out: &mut String, scalar: &Scalar<'_>) {
    match &scalar.value {
        Value::String(text) if scalar.spelling == Spelling::Bare => out.push_str(text),
        Value::String(text) => out.push_str(&strings::single_line(text)),
        Value::Number(number) => out.push_str(number.text()),
        Value::Bool(true) => out.push_str("#true"),
        Value::Bool(false) => out.push_str("#false"),
        Value::Null => out.push_str("#null"),
        _ => {}
    }
}
