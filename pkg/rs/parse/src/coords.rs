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

/// [`locate`] for many spans of one source: scanning resumes where it
/// left off, with a checkpoint every 16 KiB for earlier spans, so the
/// results never depend on call order.
#[derive(Clone, Debug)]
pub struct Locator<'s> {
    source: &'s str,
    here: Mark,
    marks: Vec<Mark>,
}

/// A scan state: a byte offset with the line, column, and code point
/// before it.
#[derive(Clone, Copy, Debug)]
struct Mark {
    at: usize,
    line: u32,
    column: u32,
    previous: Option<char>,
}

impl Mark {
    const START: Self = Self {
        at: 0,
        line: 1,
        column: 1,
        previous: None,
    };
}

/// Bytes between checkpoints.
const STRIDE: usize = 16 * 1024;

impl<'s> Locator<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            here: Mark::START,
            marks: Vec::new(),
        }
    }

    /// The span's positions, as [`locate`] resolves them.
    pub fn locate(&mut self, span: ByteSpan) -> Span {
        let start = self.position(span.start as usize);
        let end = self.position(span.end as usize);
        Span::new(start, end)
    }

    fn position(&mut self, at: usize) -> Position {
        let at = floor_boundary(self.source, at.min(self.source.len()));
        if at < self.here.at {
            let back = self.marks.partition_point(|mark| mark.at <= at);
            self.here = back.checked_sub(1).map_or(Mark::START, |i| self.marks[i]);
        }
        let mut here = self.here;
        for ch in self.source[here.at..at].chars() {
            if is_newline(ch) {
                if !(here.previous == Some('\r') && ch == '\n') {
                    here.line = here.line.saturating_add(1);
                }
                here.column = 1;
            } else {
                here.column = here.column.saturating_add(1);
            }
            here.previous = Some(ch);
            here.at += ch.len_utf8();
            let last = self.marks.last().map_or(0, |mark| mark.at);
            if here.at >= last + STRIDE {
                self.marks.push(here);
            }
        }
        self.here = here;
        Position::new(offset(at), here.line, here.column)
    }
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
