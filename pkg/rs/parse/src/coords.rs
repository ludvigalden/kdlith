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

/// [`locate`] for many spans of one source: the source's line starts
/// are indexed once, on the first lookup, so each later lookup is a
/// binary search and a count over one line's prefix. Results never
/// depend on call order.
#[derive(Clone, Debug)]
pub struct Locator<'s> {
    source: &'s str,
    /// The byte offset each line after the first starts at, ascending;
    /// `None` until the first lookup.
    lines: Option<Vec<u32>>,
    /// The latest lookup: its line's start, its offset, and its column.
    /// A later lookup further along the same line counts on from it, so
    /// a source of one very long line still resolves in one pass.
    last: (usize, usize, u32),
}

impl<'s> Locator<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            lines: None,
            last: (0, 0, 1),
        }
    }

    /// The span's positions, as [`locate`] resolves them.
    pub fn locate(&mut self, span: ByteSpan) -> Span {
        let start = self.position(span.start as usize);
        let end = self.position(span.end as usize);
        Span::new(start, end)
    }

    fn position(&mut self, at: usize) -> Position {
        let source = self.source;
        let at = floor_boundary(source, at.min(source.len()));
        let lines = self.lines.get_or_insert_with(|| line_starts(source));
        // The number of line starts at or before `at` is the number of
        // lines before its own.
        let before = lines.partition_point(|start| *start as usize <= at);
        let bytes = source.as_bytes();
        let mut start = before.checked_sub(1).map_or(0, |i| lines[i] as usize);
        // A CRLF is one newline whose line starts after the CR; the LF
        // still resets the column.
        if start > 0 && start < at && bytes[start - 1] == b'\r' && bytes[start] == b'\n' {
            start += 1;
        }
        let (from, column) = match self.last {
            (line, offset, column) if line == start && offset <= at => (offset, column),
            _ => (start, 1),
        };
        let column = column.saturating_add(scalars(&bytes[from..at]));
        self.last = (start, at, column);
        let line = u32::try_from(before).map_or(u32::MAX, |n| n.saturating_add(1));
        Position::new(offset(at), line, column)
    }
}

/// The number of Unicode scalars a UTF-8 slice between two character
/// boundaries holds: every byte that is not a continuation byte.
fn scalars(bytes: &[u8]) -> u32 {
    let count = bytes.iter().filter(|byte| (**byte & 0xC0) != 0x80).count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The byte offset each line after the first starts at: one past every
/// KDL newline, except that the LF of a CRLF starts no line of its own.
fn line_starts(source: &str) -> Vec<u32> {
    let bytes = source.as_bytes();
    let mut starts = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let width = match bytes[at] {
            b'\n' if at > 0 && bytes[at - 1] == b'\r' => {
                at += 1;
                continue;
            }
            b'\n' | b'\r' | 0x0B | 0x0C => 1,
            // U+0085.
            0xC2 if bytes.get(at + 1) == Some(&0x85) => 2,
            // U+2028 and U+2029.
            0xE2 if bytes.get(at + 1) == Some(&0x80)
                && matches!(bytes.get(at + 2), Some(0xA8 | 0xA9)) =>
            {
                3
            }
            _ => {
                at += 1;
                continue;
            }
        };
        at += width;
        starts.push(offset(at));
    }
    starts
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
