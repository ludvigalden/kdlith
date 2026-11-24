//! The check every formatted document passes before it is returned: read
//! back, it holds the same nodes, entries, values, slashdashed text, and
//! comments, in the same order, as the tree it was written from.

use kdlith_parse::tree::{Block, Comment, ItemKind, Part, Scalar};
use kdlith_parse::{Event, Parser, Value};

/// What a document means, flattened: everything but layout and
/// spelling.
#[derive(Debug, PartialEq)]
pub(crate) enum Token {
    Node {
        slashdash: bool,
        ty: Option<String>,
        name: String,
    },
    Entry {
        slashdash: bool,
        key: Option<String>,
        ty: Option<String>,
        value: String,
    },
    Open {
        slashdash: bool,
    },
    Close,
    End,
    Comment(String),
}

pub(crate) fn tokens(block: &Block<'_>) -> Vec<Token> {
    let mut out = Vec::new();
    walk(block, &mut out);
    out
}

fn text(scalar: &Scalar<'_>) -> String {
    match &scalar.value {
        Value::String(text) => format!("s:{text}"),
        Value::Number(number) => format!("n:{}", number.text()),
        Value::Bool(flag) => format!("b:{flag}"),
        Value::Null => "null".to_owned(),
        _ => "?".to_owned(),
    }
}

/// A comment's text with each line's surrounding whitespace dropped:
/// the formatter re-indents comments and trims their line ends.
fn comment(comment: &Comment<'_>) -> Token {
    let lines: Vec<&str> = comment.text.lines().map(str::trim).collect();
    Token::Comment(lines.join("\n"))
}

fn walk(block: &Block<'_>, out: &mut Vec<Token>) {
    for item in &block.items {
        match &item.kind {
            ItemKind::Comment(note) => out.push(comment(note)),
            ItemKind::Node(node) => {
                out.extend(node.leading.iter().map(comment));
                out.push(Token::Node {
                    slashdash: node.slashdash,
                    ty: node.ty.as_ref().map(text),
                    name: text(&node.name),
                });
                for part in &node.parts {
                    match part {
                        Part::Entry(entry) => out.push(Token::Entry {
                            slashdash: entry.slashdash,
                            key: entry.key.as_ref().map(text),
                            ty: entry.ty.as_ref().map(text),
                            value: text(&entry.value),
                        }),
                        Part::Comment(note) => out.push(comment(note)),
                        Part::Children(children) => {
                            out.push(Token::Open {
                                slashdash: children.slashdash,
                            });
                            walk(&children.block, out);
                            out.push(Token::Close);
                        }
                    }
                }
                out.push(Token::End);
            }
        }
    }
}

/// Whether two token streams agree, comments compared as a sequence of
/// texts apart from the structure: the formatter may move a comment
/// across a delimiter (into a block's `{` line, ahead of a node's head),
/// never drop, add, or reorder one.
pub(crate) fn same(left: &[Token], right: &[Token]) -> bool {
    split(left) == split(right)
}

/// The structure tokens and the comment tokens, each in order.
fn split(tokens: &[Token]) -> (Vec<&Token>, Vec<&Token>) {
    tokens
        .iter()
        .partition(|token| !matches!(token, Token::Comment(_)))
}

/// Whether the event parser reads the same visible document from both
/// texts: every node, entry, and value, spans aside.
pub(crate) fn same_events(left: &str, right: &str) -> bool {
    let mut a = Parser::new(left);
    let mut b = Parser::new(right);
    loop {
        match (a.next_event(), b.next_event()) {
            (Ok(None), Ok(None)) => return true,
            (Ok(Some(x)), Ok(Some(y))) => {
                if !same_event(&x.item, &y.item) {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

fn same_event(left: &Event<'_>, right: &Event<'_>) -> bool {
    left == right
}
