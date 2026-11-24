//! How a string value is spelled: bare, quoted, raw, or as a multi-line
//! string whose long lines break at spaces through whitespace escapes.
//! Every spelling here reads back as exactly the value it was made from.

use std::fmt::Write as _;

use kdlith_parse::{is_bare_string, is_disallowed, is_newline, is_space, write_string};

/// The single-line spelling of a string that is not written bare: raw
/// when the value holds a `\` or a `"` and a raw string can hold it,
/// quoted otherwise.
pub(crate) fn single_line(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    match raw_hashes(value) {
        Some(hashes) => {
            push_hashes(&mut out, hashes);
            out.push('"');
            out.push_str(value);
            out.push('"');
            push_hashes(&mut out, hashes);
        }
        None => write_string(&mut out, value),
    }
    out
}

/// A name (node name, property key, type annotation): bare when the
/// bare spelling reads back as itself, else quoted.
pub(crate) fn name(value: &str) -> String {
    if is_bare_string(value) {
        value.to_owned()
    } else {
        let mut out = String::with_capacity(value.len() + 2);
        write_string(&mut out, value);
        out
    }
}

fn push_hashes(out: &mut String, hashes: usize) {
    for _ in 0..hashes {
        out.push('#');
    }
}

/// Whether a raw string spells `value` better than a quoted one, and
/// with how many hashes: only a value holding a `\` or a `"` is worth
/// it, and only one with no code point a raw string would hold
/// invisibly or not at all.
fn raw_hashes(value: &str) -> Option<usize> {
    if !value.contains(['\\', '"']) || value.starts_with('"') {
        return None;
    }
    if value
        .chars()
        .any(|ch| is_newline(ch) || is_disallowed(ch) || (is_space(ch) && ch != ' '))
    {
        return None;
    }
    Some(1 + longest_run_after(value, "\""))
}

/// The longest run of `#` directly after any occurrence of `marker`.
fn longest_run_after(value: &str, marker: &str) -> usize {
    value
        .match_indices(marker)
        .map(|(at, _)| {
            value[at + marker.len()..]
                .bytes()
                .take_while(|byte| *byte == b'#')
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// Whether a long value may be written as a wrapped multi-line string:
/// it holds no `\` (a backslash marks code, whose text stays on one line)
/// and at least one space a line could break at.
pub(crate) fn wrappable(value: &str) -> bool {
    !value.contains('\\') && has_break(value)
}

fn has_break(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes
        .windows(2)
        .enumerate()
        .any(|(at, pair)| at > 0 && pair[0] == b' ' && pair[1] != b' ' && pair[1] != b'\t')
}

/// A multi-line string ready to place: its delimiters and its lines,
/// each line a run of segments that join through whitespace escapes.
pub(crate) struct MultiLine {
    /// The `#`s of a raw string; `None` for a quoted one.
    pub(crate) hashes: Option<usize>,
    pub(crate) lines: Vec<Line>,
    /// Prose: lines break at spaces outright, the space dropped, since a
    /// prose value reads each line break within a paragraph as a space.
    pub(crate) reflow: bool,
}

/// One line of a multi-line string's value, escaped, with the places a
/// soft break may fall.
pub(crate) struct Line {
    pub(crate) text: String,
    /// Byte offsets into `text` just after a space that is followed by
    /// text: a break there keeps the space and loses nothing.
    pub(crate) breaks: Vec<usize>,
    /// Whether the line may break at all: only a paragraph of one line
    /// does. A paragraph the author broke over several lines keeps its
    /// lines, since no word can cross a line break without changing the
    /// value.
    pub(crate) wrap: bool,
}

impl MultiLine {
    pub(crate) fn opener(&self) -> String {
        let hashes = self.hashes.unwrap_or(0);
        let mut out = String::with_capacity(hashes + 3);
        push_hashes(&mut out, hashes);
        out.push_str("\"\"\"");
        out
    }

    pub(crate) fn closer(&self) -> String {
        let hashes = self.hashes.unwrap_or(0);
        let mut out = String::with_capacity(hashes + 3);
        out.push_str("\"\"\"");
        push_hashes(&mut out, hashes);
        out
    }

    /// The body as source lines at `indent`, breaking each line of the
    /// value so that it ends at or before column `limit` where it can.
    pub(crate) fn render(&self, out: &mut String, indent: usize, limit: usize) {
        for line in &self.lines {
            if line.text.is_empty() {
                out.push('\n');
                continue;
            }
            let room = limit.saturating_sub(indent);
            let mut start = 0;
            loop {
                let rest = &line.text[start..];
                if self.reflow {
                    // The furthest break whose line, its space dropped,
                    // fits; else the nearest; else none.
                    let fits = |at: usize| width(&line.text[start..at - 1]) <= room;
                    let mut chosen = None;
                    if width(rest) > room {
                        for at in line.breaks.iter().copied().filter(|at| *at > start) {
                            if fits(at) || chosen.is_none() {
                                chosen = Some(at);
                            }
                            if !fits(at) {
                                break;
                            }
                        }
                    }
                    pad(out, indent);
                    match chosen {
                        Some(at) => {
                            out.push_str(&line.text[start..at - 1]);
                            out.push('\n');
                            start = at;
                            continue;
                        }
                        None => {
                            out.push_str(rest);
                            out.push('\n');
                            break;
                        }
                    }
                }
                if self.hashes.is_some() || !line.wrap || width(rest) <= room {
                    pad(out, indent);
                    out.push_str(rest);
                    out.push('\n');
                    break;
                }
                // The furthest break that leaves room for the `\`, else
                // the nearest one, else none.
                let candidates = line.breaks.iter().copied().filter(|at| *at > start);
                let mut chosen = None;
                for at in candidates {
                    if width(&line.text[start..at]) < room || chosen.is_none() {
                        chosen = Some(at);
                    }
                    if width(&line.text[start..at]) >= room {
                        break;
                    }
                }
                let Some(at) = chosen else {
                    pad(out, indent);
                    out.push_str(rest);
                    out.push('\n');
                    break;
                };
                pad(out, indent);
                out.push_str(&line.text[start..at]);
                out.push_str("\\\n");
                start = at;
            }
        }
    }
}

/// The multi-line form of `value`: raw when it holds a `\` and a raw
/// multi-line string can hold it as written, quoted otherwise.
pub(crate) fn multi_line(value: &str) -> MultiLine {
    if let Some(hashes) = raw_multi_hashes(value) {
        let lines = value
            .split('\n')
            .map(|text| Line {
                text: text.to_owned(),
                breaks: Vec::new(),
                wrap: false,
            })
            .collect();
        return MultiLine {
            hashes: Some(hashes),
            lines,
            reflow: false,
        };
    }
    MultiLine {
        hashes: None,
        lines: paragraphs(value.split('\n').map(quoted_line).collect()),
        reflow: false,
    }
}

/// The multi-line form of a folded prose value: each paragraph one line
/// to be filled, an empty line between paragraphs.
pub(crate) fn prose(value: &str) -> MultiLine {
    MultiLine {
        hashes: None,
        lines: value.split('\n').map(quoted_line).collect(),
        reflow: true,
    }
}

/// The multi-line form of a verbatim value: its lines as they are.
pub(crate) fn verbatim(value: &str) -> MultiLine {
    let mut string = multi_line(value);
    string.lines.iter_mut().for_each(|line| line.wrap = false);
    string
}

/// Whether a prose value has a space a line could break at.
pub(crate) fn breakable(value: &str) -> bool {
    has_break(value)
}

/// Mark every line that is a paragraph of its own (no text on the line
/// before or after it) as one that may wrap.
fn paragraphs(mut lines: Vec<Line>) -> Vec<Line> {
    let filled: Vec<bool> = lines.iter().map(|line| !line.text.is_empty()).collect();
    for (at, line) in lines.iter_mut().enumerate() {
        let before = at > 0 && filled[at - 1];
        let after = filled.get(at + 1).copied().unwrap_or(false);
        line.wrap = filled[at] && !before && !after;
    }
    lines
}

fn raw_multi_hashes(value: &str) -> Option<usize> {
    if !value.contains('\\') {
        return None;
    }
    for line in value.split('\n') {
        let trimmed = line.trim_end_matches(|ch: char| is_space(ch));
        if trimmed.len() != line.len() {
            // Trailing or whitespace-only text would not survive as written.
            return None;
        }
        if line
            .chars()
            .any(|ch| is_newline(ch) || is_disallowed(ch) || (is_space(ch) && ch != ' '))
        {
            return None;
        }
    }
    Some(1 + longest_run_after(value, "\"\"\""))
}

/// One value line of a quoted multi-line string, escaped: a `\`, a `"`
/// that would start a closing `"""`, every code point a quoted string
/// escapes, and the line's trailing whitespace, which an editor would
/// trim.
fn quoted_line(text: &str) -> Line {
    let kept = text.trim_end_matches(|ch: char| is_space(ch)).len();
    let mut out = String::with_capacity(text.len());
    let mut breaks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut previous_space = false;
    let mut seen_text = false;
    for (index, ch) in chars.iter().copied().enumerate() {
        let trailing = at >= kept;
        if previous_space && seen_text && !trailing && ch != ' ' {
            breaks.push(out.len());
        }
        previous_space = ch == ' ' && !trailing;
        seen_text |= !is_space(ch);
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' if chars.get(index + 1) == Some(&'"') && chars.get(index + 2) == Some(&'"') => {
                out.push_str("\\\"");
            }
            ' ' if trailing => out.push_str("\\s"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{C}' => out.push_str("\\f"),
            ch if is_newline(ch) || is_disallowed(ch) || (is_space(ch) && ch != ' ') => {
                let _ = write!(out, "\\u{{{:x}}}", ch as u32);
            }
            ch => out.push(ch),
        }
        at += ch.len_utf8();
    }
    Line {
        text: out,
        breaks,
        wrap: true,
    }
}

/// The display width of `text`, in Unicode scalars.
pub(crate) fn width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    }
}

pub(crate) fn pad(out: &mut String, columns: usize) {
    for _ in 0..columns {
        out.push(' ');
    }
}
