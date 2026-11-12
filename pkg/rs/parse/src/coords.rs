//! Source coordinates: byte spans resolved to lines and columns.

use std::cell::{Cell, OnceCell};

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
/// binary search and a count over one line's prefix, and a lookup on
/// the line of the one before it is the count alone. Results never
/// depend on call order.
#[derive(Clone, Debug)]
pub struct Locator<'s> {
    source: &'s str,
    /// The byte offset each line after the first starts at, ascending;
    /// unset until the first lookup.
    lines: OnceCell<Vec<u32>>,
    /// The latest lookup.
    last: Cell<Last>,
}

/// One lookup, kept so the next can start from it.
#[derive(Clone, Copy, Debug)]
struct Last {
    /// How many line starts lie at or before the offset.
    before: usize,
    /// Where the offset's line starts, and where the next one does.
    start: usize,
    end: usize,
    /// The offset and its column: a later lookup further along the
    /// line counts on from here, so a source of one very long line
    /// still resolves in one pass.
    offset: usize,
    column: u32,
}

impl<'s> Locator<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            lines: OnceCell::new(),
            last: Cell::new(Last {
                before: 0,
                start: 0,
                end: 0,
                offset: 0,
                column: 1,
            }),
        }
    }

    /// The span's positions, as [`locate`] resolves them.
    pub fn locate(&self, span: ByteSpan) -> Span {
        let start = self.position(span.start as usize);
        let end = self.position(span.end as usize);
        Span::new(start, end)
    }

    fn position(&self, at: usize) -> Position {
        let source = self.source;
        let at = floor_boundary(source, at.min(source.len()));
        let bytes = source.as_bytes();
        let last = self.last.get();
        let (before, start, end) = if last.start <= at && at < last.end {
            (last.before, last.start, last.end)
        } else {
            let lines = self.lines.get_or_init(|| line_starts(source));
            // The number of line starts at or before `at` is the number
            // of lines before its own.
            let before = lines.partition_point(|start| *start as usize <= at);
            let start = before.checked_sub(1).map_or(0, |i| lines[i] as usize);
            let end = lines.get(before).map_or(usize::MAX, |next| *next as usize);
            (before, start, end)
        };
        // A CRLF is one newline whose line starts after the CR; the LF
        // still resets the column.
        let crlf = start > 0 && start < at && bytes[start - 1] == b'\r' && bytes[start] == b'\n';
        let origin = if crlf { start + 1 } else { start };
        let (from, column) = if last.start == start && origin <= last.offset && last.offset <= at {
            (last.offset, last.column)
        } else {
            (origin, 1)
        };
        let column = column.saturating_add(scalars(&bytes[from..at]));
        self.last.set(Last {
            before,
            start,
            end,
            offset: at,
            column,
        });
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
    // Only these bytes begin a newline: the ASCII newlines, and the
    // lead bytes of U+0085, U+2028, and U+2029.
    let breaks = |byte: &u8| matches!(byte, 0x0A..=0x0D | 0xC2 | 0xE2);
    while let Some(next) = bytes[at..].iter().position(breaks) {
        at += next;
        let width = match bytes[at] {
            b'\n' if at > 0 && bytes[at - 1] == b'\r' => {
                at += 1;
                continue;
            }
            0x0A..=0x0D => 1,
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
