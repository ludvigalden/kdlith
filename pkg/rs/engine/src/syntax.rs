//! Definition documents read into an owned syntax tree, parsed once
//! through the streaming parser before compilation.

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::Value;
use kdlith_parse::{Event, Parser, Value as KdlValue};

/// One contributing definition document: its path, its text, and its
/// nodes, parsed once for the compiler.
#[derive(Clone, Debug)]
pub struct ParsedDocument {
    path: String,
    text: String,
    nodes: Vec<SyntaxNode>,
}

/// One node of a definition document with its entries and children.
#[derive(Clone, Debug)]
pub(crate) struct SyntaxNode {
    pub(crate) name: String,
    pub(crate) annotated: bool,
    pub(crate) args: Vec<Scalar>,
    pub(crate) props: Vec<(String, Scalar)>,
    pub(crate) children: Vec<SyntaxNode>,
    pub(crate) span: ByteSpan,
}

/// An argument or property value with the span of its entry.
#[derive(Clone, Debug)]
pub(crate) struct Scalar {
    pub(crate) value: Value,
    pub(crate) span: ByteSpan,
    pub(crate) annotated: bool,
}

impl ParsedDocument {
    /// Parse `text`, read from `path`. A syntax error is returned as its
    /// registered diagnostic; nothing compiles from a document that does
    /// not parse.
    pub fn parse(path: impl Into<String>, text: &str) -> Result<Self, Box<DiagnosticRecord>> {
        let path = path.into();
        let mut parser = Parser::new(text);
        let mut open: Vec<SyntaxNode> = Vec::new();
        let mut nodes = Vec::new();
        loop {
            let event = match parser.next_event() {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(error) => {
                    return Err(Box::new(error.to_diagnostic(text).with_source(path)));
                }
            };
            let span = event.span;
            match event.item {
                Event::Node { ty, name } => open.push(SyntaxNode {
                    name: name.into_owned(),
                    annotated: ty.is_some(),
                    args: Vec::new(),
                    props: Vec::new(),
                    children: Vec::new(),
                    span,
                }),
                Event::Entry(entry) => {
                    let Some(node) = open.last_mut() else {
                        continue;
                    };
                    let annotated = entry.ty.is_some();
                    let scalar = Scalar {
                        value: convert(entry.value),
                        span,
                        annotated,
                    };
                    match entry.name {
                        Some(name) => node.props.push((name.into_owned(), scalar)),
                        None => node.args.push(scalar),
                    }
                }
                Event::End => {
                    let Some(node) = open.pop() else {
                        continue;
                    };
                    match open.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => nodes.push(node),
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            path,
            text: text.to_owned(),
            nodes,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn nodes(&self) -> &[SyntaxNode] {
        &self.nodes
    }
}

fn convert(value: KdlValue<'_>) -> Value {
    match value {
        KdlValue::String(text) => Value::String(text.into_owned()),
        KdlValue::Number(number) => Value::Number(number.text().to_owned()),
        KdlValue::Bool(flag) => Value::Bool(flag),
        _ => Value::Null,
    }
}

/// A non-negative integer spelled in any KDL radix, with separators.
pub(crate) fn parse_u64(text: &str) -> Option<u64> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let (digits, radix) = if let Some(rest) = text.strip_prefix("0x") {
        (rest, 16)
    } else if let Some(rest) = text.strip_prefix("0o") {
        (rest, 8)
    } else if let Some(rest) = text.strip_prefix("0b") {
        (rest, 2)
    } else {
        (text, 10)
    };
    let cleaned: String = digits.chars().filter(|ch| *ch != '_').collect();
    if cleaned.is_empty() || !cleaned.chars().all(|ch| ch.is_digit(radix)) {
        return None;
    }
    u64::from_str_radix(&cleaned, radix).ok()
}
