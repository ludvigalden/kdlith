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
