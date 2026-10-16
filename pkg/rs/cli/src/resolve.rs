//! `--resolve-from FILE`: the CLI's own implementation of the engine's
//! value-resolution hook. Every value comes from the one file the
//! invocation names, and from nothing else — no environment variable,
//! no dotfile, no default location.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use kdlith_diagnostic::ExitStatus;
use kdlith_diagnostic::registry::{lookup, render};
use kdlith_engine::{
    MAX_DOCUMENT_BYTES, ResolutionCall, SharedResolver, Unresolvable, ValueResolver,
};
use kdlith_model::Value;
use kdlith_parse::{Event, Parser, Value as KdlValue};

use crate::read::source_error;
use crate::report::report;

/// The only node a values file holds.
const NODE: &str = "value";

/// The values one `--resolve-from` file supplies, by key.
struct FileResolver {
    shown: String,
    values: BTreeMap<String, Value>,
}

impl ValueResolver for FileResolver {
    /// A computed default names the key it is resolved by as its one
    /// dependency. The field's declared type decodes the value, and its
    /// declared sensitivity decides how it is shown.
    fn resolve(&self, call: &ResolutionCall<'_>) -> Result<Value, Unresolvable> {
        let [key] = call.dependencies else {
            return Err(Unresolvable::new(format!(
                "its computed default names {} dependencies; {} resolves exactly one",
                call.dependencies.len(),
                self.shown
            )));
        };
        self.values
            .get(key)
            .cloned()
            .ok_or_else(|| Unresolvable::new(format!("{} holds no value {key:?}", self.shown)))
    }
}

/// Load the values file `path` as the invocation's resolver. A file
/// that cannot be read or parsed fails with its source or syntax code;
/// one that breaks the values-file form is an invalid option value
/// (`KDLITH-CLI001`, exit 2).
pub(crate) fn load(path: &Path) -> Result<SharedResolver, ExitStatus> {
    let shown = path.display().to_string();
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_DOCUMENT_BYTES
    {
        let max = MAX_DOCUMENT_BYTES.to_string();
        return Err(report(&source_error(
            "KDLITH-SRC003",
            &["values file size", &max],
        )));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        report(&source_error(
            "KDLITH-SRC001",
            &[&shown, &error.to_string()],
        ))
    })?;
    let text = String::from_utf8(bytes).map_err(|error| {
        let at = error.utf8_error().valid_up_to().to_string();
        report(&source_error("KDLITH-SRC002", &[&shown, &at]))
    })?;
    let values = parse(&text, &shown)?;
    Ok(Arc::new(FileResolver { shown, values }))
}

/// Read every `value KEY VALUE` node of a values file.
fn parse(text: &str, shown: &str) -> Result<BTreeMap<String, Value>, ExitStatus> {
    let invalid = |reason: String| invalid(shown, &reason);
    let mut values = BTreeMap::new();
    let mut parser = Parser::new(text);
    let mut open: Option<Vec<Value>> = None;
    let mut key: Option<String> = None;
    loop {
        let event = match parser.next_event() {
            Ok(Some(event)) => event,
            Ok(None) => return Ok(values),
            Err(error) => return Err(report(&error.to_diagnostic(text).with_source(shown))),
        };
        let line = kdlith_parse::locate(text, event.span).start.line;
        match event.item {
            Event::Node { ty: None, name } if open.is_none() && name == NODE => {
                open = Some(Vec::new());
            }
            Event::Node { name, .. } => {
                return Err(invalid(format!(
                    "line {line}: {name:?} is not a top-level {NODE} node without a type annotation"
                )));
            }
            Event::Entry(entry) => {
                if entry.name.is_some() || entry.ty.is_some() {
                    return Err(invalid(format!(
                        "line {line}: a {NODE} node takes arguments only, without type annotations"
                    )));
                }
                let args = open.as_mut().expect("an entry belongs to an open node");
                match (args.len(), entry.value) {
                    (0, KdlValue::String(text)) => {
                        key = Some(text.into_owned());
                        args.push(Value::Null);
                    }
                    (0, _) => {
                        return Err(invalid(format!("line {line}: a {NODE} key is a string")));
                    }
                    (1, value) => match convert(value) {
                        Some(value) => args.push(value),
                        None => {
                            return Err(invalid(format!(
                                "line {line}: a {NODE} value is a string, number, or boolean"
                            )));
                        }
                    },
                    _ => {
                        return Err(invalid(format!(
                            "line {line}: a {NODE} node takes a key and one value"
                        )));
                    }
                }
            }
            Event::Children => {
                return Err(invalid(format!(
                    "line {line}: a {NODE} node takes no children"
                )));
            }
            Event::End => {
                let mut args = open.take().expect("an end closes an open node");
                let (Some(name), 2) = (key.take(), args.len()) else {
                    return Err(invalid(format!(
                        "line {line}: a {NODE} node takes a key and one value"
                    )));
                };
                let value = args.pop().expect("two arguments were read");
                if values.insert(name.clone(), value).is_some() {
                    return Err(invalid(format!(
                        "line {line}: the key {name:?} is repeated"
                    )));
                }
            }
            _ => {
                return Err(invalid(format!(
                    "line {line}: an event this version does not read"
                )));
            }
        }
    }
}

/// A KDL scalar as the bound value the resolver returns; the field's
/// declared type decodes it. A null supplies nothing, so it is refused.
fn convert(value: KdlValue<'_>) -> Option<Value> {
    match value {
        KdlValue::String(text) => Some(Value::String(text.into_owned())),
        KdlValue::Number(number) => Some(Value::Number(number.text().to_owned())),
        KdlValue::Bool(held) => Some(Value::Bool(held)),
        _ => None,
    }
}

/// A values file that breaks the form: `KDLITH-CLI001`, exit 2.
fn invalid(shown: &str, reason: &str) -> ExitStatus {
    let statement = match lookup("KDLITH-CLI001") {
        Some(definition) => render(
            definition.message_template,
            &[&format!("{shown:?}"), "--resolve-from", reason],
        ),
        None => format!("invalid value {shown:?} for --resolve-from: {reason}"),
    };
    crate::usage::report_invalid_argument(&statement)
}
