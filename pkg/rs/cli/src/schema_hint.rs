//! The `--schema` hint: one plain stderr line, never a diagnostic.
//! See [`on_failure`] for when it fires.

use std::path::Path;

use kdlith_diagnostic::ExitStatus;
use kdlith_parse::{Event, Parser};

/// Print the hint when the run failed, no schema was given, and one of
/// `entries` opens with a root node named `schema`: bound as data
/// against the governing dialect, a schema definition fails for the
/// wrong reason. `status` returns unchanged.
pub(crate) fn on_failure(
    schema: Option<&Path>,
    entries: &[&Path],
    status: ExitStatus,
) -> ExitStatus {
    if schema.is_some() || status == ExitStatus::Success {
        return status;
    }
    if let Some(path) = entries.iter().find(|path| opens_with_schema(path)) {
        eprintln!(
            "hint: {} is a schema definition; pass it as --schema {}",
            path.display(),
            path.display()
        );
    }
    status
}

/// Whether `path`'s first root node is named `schema`. The document is
/// only sniffed: an unreadable or unparseable file, or one with no
/// nodes, admits no hint, so the real failure output stands alone.
fn opens_with_schema(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(source) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let mut parser = Parser::new(source);
    loop {
        match parser.next_event() {
            // The parser emits nothing before the first node, so the
            // first Node event is the document's first root node.
            Ok(Some(event)) => {
                if let Event::Node { name, .. } = event.item {
                    return name == "schema";
                }
            }
            Ok(None) | Err(_) => return false,
        }
    }
}
