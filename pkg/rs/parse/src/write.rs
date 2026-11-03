//! Writing KDL v2: a document built node by node, rendered as text the
//! parser reads back to the same structure, in the order it was built.

use std::fmt::Write as _;

use crate::chars::{is_disallowed, is_identifier, is_newline, is_number_like, is_space};

/// A value a written entry carries.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum WriteValue {
    String(String),
    /// A string written bare when it reads back as itself, as an
    /// identifier-like value (`kind=fragment`) is; quoted otherwise.
    Identifier(String),
    /// A number as spelled; the caller vouches it is a KDL number.
    Number(String),
    Bool(bool),
    Null,
}

impl WriteValue {
    /// A string value written bare when the bare spelling reads back as
    /// the same string.
    pub fn identifier(text: impl Into<String>) -> Self {
        Self::Identifier(text.into())
    }
}

impl From<&str> for WriteValue {
    fn from(text: &str) -> Self {
        Self::String(text.to_owned())
    }
}

impl From<String> for WriteValue {
    fn from(text: String) -> Self {
        Self::String(text)
    }
}

impl From<bool> for WriteValue {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<u64> for WriteValue {
    fn from(number: u64) -> Self {
        Self::Number(number.to_string())
    }
}

impl From<usize> for WriteValue {
    fn from(number: usize) -> Self {
        Self::Number(number.to_string())
    }
}

impl From<i64> for WriteValue {
    fn from(number: i64) -> Self {
        Self::Number(number.to_string())
    }
}

/// One line of a written document: a node, or a `//` comment.
#[derive(Clone, Debug, PartialEq)]
enum Line {
    Node(WriteNode),
    Comment(String),
}

/// A node to write: its name, arguments and properties in order, and
/// its children.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteNode {
    name: String,
    entries: Vec<(Option<String>, WriteValue)>,
    children: Vec<Line>,
    /// Whether the node writes a children block even when it has none.
    block: bool,
}

impl WriteNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
            children: Vec::new(),
            block: false,
        }
    }

    /// Append an argument.
    #[must_use]
    pub fn arg(mut self, value: impl Into<WriteValue>) -> Self {
        self.entries.push((None, value.into()));
        self
    }

    /// Append a property.
    #[must_use]
    pub fn prop(mut self, name: impl Into<String>, value: impl Into<WriteValue>) -> Self {
        self.entries.push((Some(name.into()), value.into()));
        self
    }

    /// Append a child node.
    #[must_use]
    pub fn child(mut self, child: WriteNode) -> Self {
        self.children.push(Line::Node(child));
        self
    }

    /// Append a `//` comment line among the children.
    #[must_use]
    pub fn comment(mut self, text: impl Into<String>) -> Self {
        self.children.push(Line::Comment(text.into()));
        self
    }

    /// Write `{ }` even when the node has no children.
    #[must_use]
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }

    fn render(&self, out: &mut String, depth: usize, indent: &str) {
        for _ in 0..depth {
            out.push_str(indent);
        }
        write_name(out, &self.name);
        for (name, value) in &self.entries {
            out.push(' ');
            if let Some(name) = name {
                write_name(out, name);
                out.push('=');
            }
            write_value(out, value);
        }
        if self.children.is_empty() && !self.block {
            out.push('\n');
            return;
        }
        out.push_str(" {\n");
        for line in &self.children {
            render_line(line, out, depth + 1, indent);
        }
        for _ in 0..depth {
            out.push_str(indent);
        }
        out.push_str("}\n");
    }
}

fn render_line(line: &Line, out: &mut String, depth: usize, indent: &str) {
    match line {
        Line::Node(node) => node.render(out, depth, indent),
        Line::Comment(text) => {
            for part in text.split('\n') {
                for _ in 0..depth {
                    out.push_str(indent);
                }
                if part.is_empty() {
                    out.push_str("//\n");
                } else {
                    let _ = writeln!(out, "// {part}");
                }
            }
        }
    }
}

/// A document to write: top-level nodes and comments, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WriteDocument {
    lines: Vec<Line>,
    indent: Option<String>,
}

impl WriteDocument {
    pub fn new() -> Self {
        Self::default()
    }

    /// Indent each nesting level with `indent` (default four spaces).
    #[must_use]
    pub fn with_indent(mut self, indent: impl Into<String>) -> Self {
        self.indent = Some(indent.into());
        self
    }

    /// Append a top-level node.
    pub fn node(&mut self, node: WriteNode) -> &mut Self {
        self.lines.push(Line::Node(node));
        self
    }

    /// Append a top-level `//` comment; each line of `text` becomes one
    /// comment line.
    pub fn comment(&mut self, text: impl Into<String>) -> &mut Self {
        self.lines.push(Line::Comment(text.into()));
        self
    }

    /// The document as KDL v2 text, ending in a newline when it holds
    /// anything.
    pub fn render(&self) -> String {
        let indent = self.indent.as_deref().unwrap_or("    ");
        let mut out = String::new();
        for line in &self.lines {
            render_line(line, &mut out, 0, indent);
        }
        out
    }
}

/// Write `value` as a KDL value.
pub fn write_value(out: &mut String, value: &WriteValue) {
    match value {
        WriteValue::String(text) => write_string(out, text),
        WriteValue::Identifier(text) => write_name(out, text),
        WriteValue::Number(spelled) => out.push_str(spelled),
        WriteValue::Bool(true) => out.push_str("#true"),
        WriteValue::Bool(false) => out.push_str("#false"),
        WriteValue::Null => out.push_str("#null"),
    }
}

/// Write `text` as a KDL string: quoted, with `"` and `\` escaped,
/// newlines and the other control characters a quoted string cannot
/// hold written as escapes, and every other character as itself. The
/// parser reads the result back as exactly `text`.
pub fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{C}' => out.push_str("\\f"),
            ch if is_newline(ch) || is_disallowed(ch) || (is_space(ch) && ch != ' ') => {
                let _ = write!(out, "\\u{{{:x}}}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Write `name` as a node or property name: bare when it reads back as
/// itself, else as a quoted string.
pub fn write_name(out: &mut String, name: &str) {
    if is_bare_string(name) {
        out.push_str(name);
    } else {
        write_string(out, name);
    }
}

/// Whether `text` can be written bare: it reads back as the same
/// string, never as a number or a keyword.
pub fn is_bare_string(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(is_identifier)
        && !is_number_like(text)
        && !matches!(text, "true" | "false" | "null" | "inf" | "-inf" | "nan")
}
