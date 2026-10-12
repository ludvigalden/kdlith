//! Parse errors and their mapping onto registered diagnostics.

use std::fmt;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};

use crate::coords::locate;

/// Why a parse stopped. Parsing is fail-closed: the first error ends the
/// event stream.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseError {
    kind: ErrorKind,
    span: ByteSpan,
}

/// The condition behind a [`ParseError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A code point KDL forbids anywhere in a document.
    DisallowedCharacter(char),
    /// A code point that cannot start or continue the construct here.
    UnexpectedCharacter(char),
    /// The input ended inside a construct that is not a string, a comment,
    /// or a children block.
    UnexpectedEnd,
    /// A string, comment, or children block reached the end of input.
    Unterminated(Construct),
    /// Whitespace is required before an argument or property.
    ExpectedSpace,
    /// A node name, type annotation, or property key must be a string.
    ExpectedString,
    /// A type annotation is missing its `)`.
    ExpectedCloseParen,
    /// A property `=` is not followed by a value.
    ExpectedValue,
    /// A multi-line string opener or a line continuation needs a newline.
    ExpectedNewline,
    /// A single-line string contains a literal newline.
    UnexpectedNewline,
    InvalidNumber,
    /// A `#` keyword other than `#true`, `#false`, `#null`, `#inf`,
    /// `#-inf`, or `#nan`.
    InvalidKeyword,
    /// A keyword spelled without its `#`, such as a bare `true`.
    BareKeyword,
    InvalidEscape,
    /// A multi-line string line does not start with the closing line's
    /// indentation.
    InvalidIndent,
    /// A multi-line string's closing line holds text before the delimiter.
    TextBeforeClose,
    /// A `}` with no open children block.
    UnmatchedClose,
    /// An argument or property follows a children block.
    EntryAfterChildren,
    /// A second children block that is not slashdashed.
    MultipleChildren,
    /// A configured resource limit; the parse stopped before growing past it.
    LimitExceeded {
        limit: Limit,
        max: usize,
    },
}

/// A construct that can reach the end of input unterminated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Construct {
    String,
    Comment,
    Children,
}

/// A resource limit from [`crate::Limits`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
    SourceBytes,
    TokenBytes,
    StringLines,
    Depth,
    CommentDepth,
}

impl ParseError {
    pub(crate) fn new(kind: ErrorKind, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: ByteSpan::new(offset(start), offset(end)),
        }
    }

    pub(crate) fn at(kind: ErrorKind, at: usize) -> Self {
        Self::new(kind, at, at)
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn span(&self) -> ByteSpan {
        self.span
    }

    /// The registered diagnostic code: `KDLITH-SRC003` for a limit breach,
    /// `KDLITH-SYN002` for an unterminated construct, `KDLITH-SYN001`
    /// otherwise.
    pub fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::LimitExceeded { .. } => "KDLITH-SRC003",
            ErrorKind::Unterminated(_) => "KDLITH-SYN002",
            _ => "KDLITH-SYN001",
        }
    }

    /// The diagnostic record for this error, with its message filled and
    /// its span located in `source`, the text that was parsed.
    pub fn to_diagnostic(&self, source: &str) -> DiagnosticRecord {
        let code = self.code();
        let mut record = DiagnosticRecord::from_code(code)
            .expect("parse error codes are registered and never retired");
        if let Some(definition) = lookup(code) {
            let message = match self.kind {
                ErrorKind::LimitExceeded { limit, max } => render(
                    definition.message_template,
                    &[limit.name(), &max.to_string()],
                ),
                ErrorKind::Unterminated(construct) => {
                    render(definition.message_template, &[construct.name()])
                }
                kind => render(definition.message_template, &[&kind.to_string()]),
            };
            record = record.with_message(message);
        }
        let start = locate(source, self.span).start;
        record.with_span(self.span, Some(start.line), Some(start.column))
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.span.start)
    }
}

impl std::error::Error for ParseError {}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::DisallowedCharacter(ch) => write!(f, "disallowed character {ch:?}"),
            ErrorKind::UnexpectedCharacter(ch) => write!(f, "unexpected character {ch:?}"),
            ErrorKind::UnexpectedEnd => f.write_str("unexpected end of input"),
            ErrorKind::Unterminated(construct) => write!(f, "unterminated {}", construct.name()),
            ErrorKind::ExpectedSpace => f.write_str("expected whitespace before entry"),
            ErrorKind::ExpectedString => f.write_str("expected a string"),
            ErrorKind::ExpectedCloseParen => f.write_str("expected ')'"),
            ErrorKind::ExpectedValue => f.write_str("expected a value"),
            ErrorKind::ExpectedNewline => f.write_str("expected a newline"),
            ErrorKind::UnexpectedNewline => f.write_str("newline in single-line string"),
            ErrorKind::InvalidNumber => f.write_str("invalid number"),
            ErrorKind::InvalidKeyword => f.write_str("invalid keyword"),
            ErrorKind::BareKeyword => f.write_str("keyword used without '#'"),
            ErrorKind::InvalidEscape => f.write_str("invalid string escape"),
            ErrorKind::InvalidIndent => f.write_str("multi-line string indentation mismatch"),
            ErrorKind::TextBeforeClose => f.write_str("text before multi-line string close"),
            ErrorKind::UnmatchedClose => f.write_str("unmatched '}'"),
            ErrorKind::EntryAfterChildren => f.write_str("entry after children block"),
            ErrorKind::MultipleChildren => f.write_str("more than one children block"),
            ErrorKind::LimitExceeded { limit, max } => {
                write!(f, "{} limit {max} exceeded", limit.name())
            }
        }
    }
}

impl Construct {
    fn name(self) -> &'static str {
        match self {
            Construct::String => "string",
            Construct::Comment => "block comment",
            Construct::Children => "children block",
        }
    }
}

impl Limit {
    fn name(self) -> &'static str {
        match self {
            Limit::SourceBytes => "source size",
            Limit::TokenBytes => "token size",
            Limit::StringLines => "multi-line string lines",
            Limit::Depth => "nesting depth",
            Limit::CommentDepth => "comment nesting depth",
        }
    }
}

/// A byte offset as carried in spans; sources longer than `u32::MAX`
/// bytes are rejected before parsing starts.
#[expect(
    clippy::cast_possible_truncation,
    reason = "offsets are bounded by the source size check"
)]
pub(crate) fn offset(at: usize) -> u32 {
    at as u32
}
