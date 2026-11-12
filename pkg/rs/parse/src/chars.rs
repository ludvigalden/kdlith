//! KDL code-point classes.

/// `disallowed-literal-code-points`; surrogates cannot occur in a `char`.
pub(crate) fn is_disallowed(ch: char) -> bool {
    matches!(
        ch,
        '\u{0}'..='\u{8}'
            | '\u{E}'..='\u{1F}'
            | '\u{7F}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// `unicode-space`.
pub(crate) fn is_space(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}' | '\u{20}' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// `newline`; CRLF is two newline code points that count as one line.
pub(crate) fn is_newline(ch: char) -> bool {
    matches!(ch, '\u{A}'..='\u{D}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// What an ASCII byte is to the scanner's hot loops, by its code. A
/// byte at or above `0x80` starts a longer code point, which the
/// code-point classes above decide.
const ASCII: [u8; 128] = ascii_classes();

/// A space or a tab.
const SPACE: u8 = 1;
/// A newline code point.
const NEWLINE: u8 = 2;
/// A disallowed literal code point.
const DISALLOWED: u8 = 4;
/// A code point no identifier holds besides the classes above.
const PUNCTUATION: u8 = 8;
/// A string delimiter or escape: `"` or `\`.
const QUOTING: u8 = 16;

const fn ascii_classes() -> [u8; 128] {
    let mut classes = [0_u8; 128];
    let mut code = 0;
    while code < 128 {
        let byte = code as u8;
        let mut class = 0;
        if matches!(byte, 0x09 | 0x20) {
            class |= SPACE;
        }
        if matches!(byte, 0x0A..=0x0D) {
            class |= NEWLINE;
        }
        if matches!(byte, 0x00..=0x08 | 0x0E..=0x1F | 0x7F) {
            class |= DISALLOWED;
        }
        if matches!(
            byte,
            b'\\' | b'/' | b'(' | b')' | b'{' | b'}' | b';' | b'[' | b']' | b'"' | b'#' | b'='
        ) {
            class |= PUNCTUATION;
        }
        if matches!(byte, b'"' | b'\\') {
            class |= QUOTING;
        }
        classes[code] = class;
        code += 1;
    }
    classes
}

/// How many leading bytes of `bytes` are ASCII holding none of the
/// classes in `stop`.
fn ascii_run(bytes: &[u8], stop: u8) -> usize {
    bytes
        .iter()
        .position(|byte| *byte >= 0x80 || ASCII[*byte as usize] & stop != 0)
        .unwrap_or(bytes.len())
}

/// The leading ASCII identifier characters of `bytes`.
pub(crate) fn identifier_run(bytes: &[u8]) -> usize {
    ascii_run(bytes, SPACE | NEWLINE | DISALLOWED | PUNCTUATION)
}

/// The leading ASCII characters of `bytes` a single-line string holds
/// as written: neither a delimiter, an escape, a newline, nor a
/// disallowed code point.
pub(crate) fn string_run(bytes: &[u8]) -> usize {
    ascii_run(bytes, NEWLINE | DISALLOWED | QUOTING)
}

/// The leading ASCII characters of `bytes` a comment or a multi-line
/// string line holds: neither a newline nor a disallowed code point.
pub(crate) fn text_run(bytes: &[u8]) -> usize {
    ascii_run(bytes, NEWLINE | DISALLOWED)
}

/// The leading spaces and tabs of `bytes`.
pub(crate) fn space_run(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !matches!(byte, 0x09 | 0x20))
        .unwrap_or(bytes.len())
}

/// The leading spaces, tabs, and ASCII newlines of `bytes`.
pub(crate) fn blank_run(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !matches!(byte, 0x09..=0x0D | 0x20))
        .unwrap_or(bytes.len())
}

/// `identifier-char`.
pub(crate) fn is_identifier(ch: char) -> bool {
    !(is_disallowed(ch)
        || is_space(ch)
        || is_newline(ch)
        || matches!(
            ch,
            '\\' | '/' | '(' | ')' | '{' | '}' | ';' | '[' | ']' | '"' | '#' | '='
        ))
}

/// Whether an identifier would read as a number: a digit after an
/// optional sign and an optional dot.
pub(crate) fn is_number_like(text: &str) -> bool {
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let text = text.strip_prefix('.').unwrap_or(text);
    text.as_bytes().first().is_some_and(u8::is_ascii_digit)
}
