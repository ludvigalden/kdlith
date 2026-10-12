//! Source coordinates: byte spans resolved to lines and columns.

use kdlith_diagnostic::ByteSpan;
use kdlith_model::{Position, Span};

use crate::chars::is_newline;
use crate::error::offset;

/// Resolve a byte span over `source` to positions with 1-based lines and
/// 1-based Unicode-scalar columns. Lines break at every KDL newline, with
/// CRLF counted once; offsets past the end clamp to the end.
pub fn locate(source: &str, span: ByteSpan) -> Span {
    let start = position(source, span.start as usize);
    let end = position(source, span.end as usize);
    Span::new(start, end)
}

fn position(source: &str, at: usize) -> Position {
    let at = floor_boundary(source, at.min(source.len()));
    let mut line = 1_u32;
    let mut column = 1_u32;
    let mut previous = None;
    for ch in source[..at].chars() {
        if is_newline(ch) {
            if !(previous == Some('\r') && ch == '\n') {
                line = line.saturating_add(1);
            }
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
        previous = Some(ch);
    }
    Position::new(offset(at), line, column)
}

fn floor_boundary(source: &str, mut at: usize) -> usize {
    while !source.is_char_boundary(at) {
        at -= 1;
    }
    at
}
