//! Grammar productions over a borrowed source: whitespace, comments,
//! strings, numbers, and keywords.

use std::borrow::Cow;

use crate::chars::{is_disallowed, is_identifier, is_newline, is_number_like, is_space};
use crate::error::{Construct, ErrorKind, Limit, ParseError};
use crate::event::{Number, NumberKind, Value};
use crate::limits::Limits;
use crate::number::classify;

pub(crate) type Result<T> = std::result::Result<T, ParseError>;

/// A cursor over the source. Every production advances `at` past what it
/// accepted and leaves it at the first byte it rejected.
pub(crate) struct Scanner<'src> {
    src: &'src str,
    at: usize,
    limits: Limits,
    /// Skip mode, for slashdashed text whose values are never emitted:
    /// strings are fully validated but not decoded.
    skip: bool,
}

/// One line of a multi-line string, as offsets into the decoded buffer.
struct Line {
    source_at: usize,
    start: usize,
    text: usize,
    end: usize,
}

impl<'src> Scanner<'src> {
    pub(crate) fn new(src: &'src str, limits: Limits) -> Self {
        Self {
            src,
            at: 0,
            limits,
            skip: false,
        }
    }

    pub(crate) fn set_skip(&mut self, skip: bool) {
        self.skip = skip;
    }

    pub(crate) fn at(&self) -> usize {
        self.at
    }

    pub(crate) fn source_len(&self) -> usize {
        self.src.len()
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.src[self.at..].chars().next()
    }

    fn starts_with(&self, text: &str) -> bool {
        self.src[self.at..].starts_with(text)
    }

    fn bump(&mut self, ch: char) {
        self.at += ch.len_utf8();
    }

    pub(crate) fn bump_byte(&mut self) {
        self.at += 1;
    }

    /// The error for the code point at the cursor, which no production
    /// accepts here.
    pub(crate) fn unexpected(&self) -> ParseError {
        match self.peek() {
            None => ParseError::at(ErrorKind::UnexpectedEnd, self.at),
            Some(ch) if is_disallowed(ch) => self.error_here(ErrorKind::DisallowedCharacter(ch)),
            Some(ch) => self.error_here(ErrorKind::UnexpectedCharacter(ch)),
        }
    }

    /// An error spanning the code point at the cursor.
    pub(crate) fn error_here(&self, kind: ErrorKind) -> ParseError {
        let width = self.peek().map_or(0, char::len_utf8);
        ParseError::new(kind, self.at, self.at + width)
    }

    fn check_token(&self, start: usize) -> Result<()> {
        let max = self.limits.max_token_bytes;
        if self.at - start > max {
            let kind = ErrorKind::LimitExceeded {
                limit: Limit::TokenBytes,
                max,
            };
            return Err(ParseError::new(kind, start, self.at));
        }
        Ok(())
    }

    /// `bom`, accepted only as the first code point.
    pub(crate) fn bom(&mut self) {
        if self.at == 0 && self.starts_with("\u{FEFF}") {
            self.bump('\u{FEFF}');
        }
    }

    /// One `newline`, with CRLF taken as a single newline.
    pub(crate) fn newline(&mut self) -> bool {
        match self.peek() {
            Some('\r') => {
                self.bump('\r');
                if self.peek() == Some('\n') {
                    self.bump('\n');
                }
                true
            }
            Some(ch) if is_newline(ch) => {
                self.bump(ch);
                true
            }
            _ => false,
        }
    }

    /// `single-line-comment` after its `//`, through its newline.
    pub(crate) fn line_comment(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                None => return Ok(()),
                Some(ch) if is_newline(ch) => {
                    self.newline();
                    return Ok(());
                }
                Some(ch) if is_disallowed(ch) => {
                    return Err(self.error_here(ErrorKind::DisallowedCharacter(ch)));
                }
                Some(ch) => self.bump(ch),
            }
        }
    }

    /// `multi-line-comment` after its `/*`; comments nest.
    fn block_comment(&mut self, start: usize) -> Result<()> {
        let mut depth = 0_usize;
        loop {
            if self.starts_with("*/") {
                self.at += 2;
                match depth.checked_sub(1) {
                    Some(outer) => depth = outer,
                    None => return Ok(()),
                }
            } else if self.starts_with("/*") {
                let max = self.limits.max_comment_depth;
                if depth >= max {
                    let kind = ErrorKind::LimitExceeded {
                        limit: Limit::CommentDepth,
                        max,
                    };
                    return Err(ParseError::new(kind, self.at, self.at + 2));
                }
                self.at += 2;
                depth += 1;
            } else {
                match self.peek() {
                    None => {
                        let kind = ErrorKind::Unterminated(Construct::Comment);
                        return Err(ParseError::new(kind, start, self.at));
                    }
                    Some(ch) if is_disallowed(ch) => {
                        return Err(self.error_here(ErrorKind::DisallowedCharacter(ch)));
                    }
                    Some(ch) => self.bump(ch),
                }
            }
        }
    }

    /// `escline` after its `\`: whitespace, then a line comment, a
    /// newline, or the end of input.
    fn escline(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(ch) if is_space(ch) => self.bump(ch),
                Some('/') if self.starts_with("/*") => {
                    let start = self.at;
                    self.at += 2;
                    self.block_comment(start)?;
                }
                _ => break,
            }
        }
        if self.starts_with("//") {
            self.at += 2;
            return self.line_comment();
        }
        if self.peek().is_none() || self.newline() {
            return Ok(());
        }
        Err(self.error_here(ErrorKind::ExpectedNewline))
    }

    /// `node-space*`; reports whether any was consumed.
    pub(crate) fn node_space(&mut self) -> Result<bool> {
        let start = self.at;
        loop {
            match self.peek() {
                Some(ch) if is_space(ch) => self.bump(ch),
                Some('\\') => {
                    self.bump('\\');
                    self.escline()?;
                }
                Some('/') if self.starts_with("/*") => {
                    let comment = self.at;
                    self.at += 2;
                    self.block_comment(comment)?;
                }
                _ => return Ok(self.at != start),
            }
        }
    }

    /// `line-space*`.
    pub(crate) fn line_space(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(ch) if is_space(ch) => self.bump(ch),
                Some(ch) if is_newline(ch) => {
                    self.newline();
                }
                Some('\\') => {
                    self.bump('\\');
                    self.escline()?;
                }
                Some('/') if self.starts_with("//") => {
                    self.at += 2;
                    self.line_comment()?;
                }
                Some('/') if self.starts_with("/*") => {
                    let comment = self.at;
                    self.at += 2;
                    self.block_comment(comment)?;
                }
                _ => return Ok(()),
            }
        }
    }

    /// Whether the cursor is at a `//` line comment.
    pub(crate) fn at_line_comment(&self) -> bool {
        self.starts_with("//")
    }

    /// `slashdash`: `/-` and the line space after it.
    pub(crate) fn slashdash(&mut self) -> Result<bool> {
        if !self.starts_with("/-") {
            return Ok(false);
        }
        self.at += 2;
        self.line_space()?;
        Ok(true)
    }

    /// `type?`: a parenthesized string, with node space inside.
    pub(crate) fn type_annotation(&mut self) -> Result<Option<Cow<'src, str>>> {
        if self.peek() != Some('(') {
            return Ok(None);
        }
        self.bump('(');
        self.node_space()?;
        let ty = self.string()?;
        self.node_space()?;
        if self.peek() != Some(')') {
            return Err(self.error_here(ErrorKind::ExpectedCloseParen));
        }
        self.bump(')');
        Ok(Some(ty))
    }

    /// `string`: an identifier, quoted, or raw string.
    pub(crate) fn string(&mut self) -> Result<Cow<'src, str>> {
        let start = self.at;
        match self.value()? {
            Value::String(text) => Ok(text),
            _ => Err(ParseError::new(ErrorKind::ExpectedString, start, self.at)),
        }
    }

    /// `value`: a string, number, or keyword.
    pub(crate) fn value(&mut self) -> Result<Value<'src>> {
        let start = self.at;
        match self.peek() {
            Some('"') => {
                self.bump('"');
                self.quoted(start, 0).map(Value::String)
            }
            Some('#') => {
                self.bump('#');
                match self.peek() {
                    Some(ch) if is_identifier(ch) => self.keyword(start),
                    _ => self.raw(start),
                }
            }
            Some(ch) if is_identifier(ch) => self.bare(start),
            _ => Err(self.unexpected()),
        }
    }

    fn identifier(&mut self, start: usize) -> Result<&'src str> {
        while let Some(ch) = self.peek() {
            if !is_identifier(ch) {
                break;
            }
            self.bump(ch);
            self.check_token(start)?;
        }
        Ok(&self.src[start..self.at])
    }

    fn bare(&mut self, start: usize) -> Result<Value<'src>> {
        let text = self.identifier(start)?;
        if is_number_like(text) {
            let kind = classify(text)
                .ok_or_else(|| ParseError::new(ErrorKind::InvalidNumber, start, self.at))?;
            return Ok(Value::Number(Number::new(text, kind)));
        }
        if matches!(text, "true" | "false" | "null" | "inf" | "-inf" | "nan") {
            return Err(ParseError::new(ErrorKind::BareKeyword, start, self.at));
        }
        Ok(Value::String(Cow::Borrowed(text)))
    }

    /// `keyword` after its `#`.
    fn keyword(&mut self, start: usize) -> Result<Value<'src>> {
        let word = self.identifier(self.at)?;
        let text = &self.src[start..self.at];
        let number = |kind| Value::Number(Number::new(text, kind));
        Ok(match word {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            "inf" => number(NumberKind::Infinity),
            "-inf" => number(NumberKind::NegativeInfinity),
            "nan" => number(NumberKind::NotANumber),
            _ => return Err(ParseError::new(ErrorKind::InvalidKeyword, start, self.at)),
        })
    }

    /// A raw string after its first `#`.
    fn raw(&mut self, start: usize) -> Result<Value<'src>> {
        let mut hashes = 1;
        while self.peek() == Some('#') {
            self.bump('#');
            hashes += 1;
            self.check_token(start)?;
        }
        if self.peek() != Some('"') {
            return Err(ParseError::new(ErrorKind::ExpectedString, start, self.at));
        }
        self.bump('"');
        self.quoted(start, hashes).map(Value::String)
    }

    /// Whether the cursor is at a closing delimiter: `quotes` quotes and
    /// then `hashes` hashes.
    fn closes(&self, quotes: usize, hashes: usize) -> bool {
        let rest = &self.src.as_bytes()[self.at..];
        rest.len() >= quotes + hashes
            && rest[..quotes].iter().all(|&byte| byte == b'"')
            && rest[quotes..quotes + hashes]
                .iter()
                .all(|&byte| byte == b'#')
    }

    /// A quoted or raw string after its opening quote.
    fn quoted(&mut self, start: usize, hashes: usize) -> Result<Cow<'src, str>> {
        if self.starts_with("\"\"") {
            self.at += 2;
            if !self.newline() {
                return Err(self.error_here(ErrorKind::ExpectedNewline));
            }
            return self.multi_line(start, hashes);
        }
        self.single_line(start, hashes)
    }

    fn unterminated(&self, start: usize) -> ParseError {
        ParseError::new(ErrorKind::Unterminated(Construct::String), start, self.at)
    }

    fn single_line(&mut self, start: usize, hashes: usize) -> Result<Cow<'src, str>> {
        let body = self.at;
        let mut decoded: Option<String> = None;
        loop {
            self.check_token(start)?;
            match self.peek() {
                None => return Err(self.unterminated(start)),
                Some('"') if self.closes(1, hashes) => {
                    let text = &self.src[body..self.at];
                    self.at += 1 + hashes;
                    return Ok(decoded.map_or(Cow::Borrowed(text), Cow::Owned));
                }
                Some('\\') if hashes == 0 => {
                    let prefix = &self.src[body..self.at];
                    self.bump('\\');
                    let escaped = self.escape(start)?;
                    if !self.skip {
                        let decoded = decoded.get_or_insert_with(|| prefix.to_owned());
                        decoded.extend(escaped);
                    }
                }
                Some(ch) if is_newline(ch) => {
                    return Err(self.error_here(ErrorKind::UnexpectedNewline));
                }
                Some(ch) if is_disallowed(ch) => {
                    return Err(self.error_here(ErrorKind::DisallowedCharacter(ch)));
                }
                Some(ch) => {
                    if let Some(decoded) = &mut decoded {
                        decoded.push(ch);
                    }
                    self.bump(ch);
                }
            }
        }
    }

    /// A string escape after its `\`; whitespace escapes yield `None`.
    fn escape(&mut self, start: usize) -> Result<Option<char>> {
        let escape = self.at - 1;
        let Some(written) = self.peek() else {
            return Err(self.unterminated(start));
        };
        let decoded = match written {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '\\' => '\\',
            '"' => '"',
            'b' => '\u{8}',
            'f' => '\u{C}',
            's' => ' ',
            'u' => return self.unicode_escape(escape).map(Some),
            ch if is_space(ch) || is_newline(ch) => {
                while let Some(ch) = self.peek() {
                    if !is_space(ch) && !is_newline(ch) {
                        break;
                    }
                    self.bump(ch);
                }
                return Ok(None);
            }
            ch => {
                self.bump(ch);
                return Err(ParseError::new(ErrorKind::InvalidEscape, escape, self.at));
            }
        };
        self.bump(written);
        Ok(Some(decoded))
    }

    /// `\u{...}` after its `\`: one to six hex digits naming a scalar value.
    fn unicode_escape(&mut self, escape: usize) -> Result<char> {
        let invalid = |at: usize| ParseError::new(ErrorKind::InvalidEscape, escape, at);
        self.bump('u');
        if self.peek() != Some('{') {
            return Err(invalid(self.at));
        }
        self.bump('{');
        let digits = self.at;
        while self.at - digits < 6 && self.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
            self.at += 1;
        }
        let value = u32::from_str_radix(&self.src[digits..self.at], 16).ok();
        if self.peek() != Some('}') {
            return Err(invalid(self.at));
        }
        self.bump('}');
        value
            .and_then(char::from_u32)
            .ok_or_else(|| invalid(self.at))
    }

    /// A multi-line string after its opening newline. Each line's
    /// indentation must start with the closing line's, which is removed;
    /// skip mode checks it but never builds the dedented value.
    fn multi_line(&mut self, start: usize, hashes: usize) -> Result<Cow<'src, str>> {
        let mut decoded = String::new();
        let mut lines: Vec<Option<Line>> = Vec::new();
        let indent = 'lines: loop {
            if lines.len() >= self.limits.max_string_lines {
                let kind = ErrorKind::LimitExceeded {
                    limit: Limit::StringLines,
                    max: self.limits.max_string_lines,
                };
                return Err(ParseError::new(kind, start, self.at));
            }
            let source_at = self.at;
            let line_start = decoded.len();
            while let Some(ch) = self.peek().filter(|&ch| is_space(ch)) {
                self.check_token(start)?;
                decoded.push(ch);
                self.bump(ch);
            }
            let text = decoded.len();
            if self.newline() {
                lines.push(None);
                continue;
            }
            // Whether the line holds text after its indentation; in skip
            // mode that text is validated but never buffered.
            let mut written = false;
            loop {
                self.check_token(start)?;
                match self.peek() {
                    None => return Err(self.unterminated(start)),
                    Some('"') if self.closes(3, hashes) => {
                        self.at += 3 + hashes;
                        if written {
                            return Err(ParseError::new(
                                ErrorKind::TextBeforeClose,
                                source_at,
                                self.at,
                            ));
                        }
                        break 'lines line_start..text;
                    }
                    Some(ch) if is_newline(ch) => {
                        lines.push(Some(Line {
                            source_at,
                            start: line_start,
                            text,
                            end: decoded.len(),
                        }));
                        self.newline();
                        break;
                    }
                    Some('\\') if hashes == 0 => {
                        self.bump('\\');
                        if let Some(ch) = self.escape(start)? {
                            written = true;
                            if !self.skip {
                                decoded.push(ch);
                            }
                        }
                    }
                    Some(ch) if is_disallowed(ch) => {
                        return Err(self.error_here(ErrorKind::DisallowedCharacter(ch)));
                    }
                    Some(ch) => {
                        written = true;
                        if !self.skip {
                            decoded.push(ch);
                        }
                        self.bump(ch);
                    }
                }
            }
        };
        let prefix = &decoded[indent.clone()];
        let mut out = String::with_capacity(if self.skip { 0 } else { decoded.len() });
        for (index, line) in lines.iter().enumerate() {
            if index > 0 && !self.skip {
                out.push('\n');
            }
            let Some(line) = line else { continue };
            let leading = &decoded[line.start..line.text];
            if !leading.starts_with(prefix) {
                let kind = ErrorKind::InvalidIndent;
                return Err(ParseError::new(kind, line.source_at, line.source_at));
            }
            if !self.skip {
                out.push_str(&decoded[line.start + prefix.len()..line.end]);
            }
        }
        Ok(if self.skip {
            Cow::Borrowed("")
        } else {
            Cow::Owned(out)
        })
    }
}
