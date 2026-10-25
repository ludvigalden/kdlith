//! The regex primitive the schema surface names: the pinned type, and
//! the whole-string matching discipline the engine's pattern
//! constraints compile with.
pub use regex::Regex;

/// Whether `line` matches `pattern` in full: the pattern must cover the
/// whole line, never a fragment, exactly as the engine's pattern
/// constraints require. A pattern that fails to compile matches nothing.
pub fn matches_line(pattern: &str, line: &str) -> bool {
    Regex::new(&format!("^(?:{pattern})$")).is_ok_and(|regex| regex.is_match(line))
}
