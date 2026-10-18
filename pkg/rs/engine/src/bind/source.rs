//! The parser adapter: the streaming parser's pull events mapped onto
//! the runtime's event-source contract, names borrowed from the source.

use kdlith_model::Value;
use kdlith_parse::{Event, Parser, Value as KdlValue};
use kdlith_runtime::{EventSource, Pulled, SourceEvent, Step};

use super::{EntryValue, NodeStart};

/// The pull events of one document, ready for a binding session. A parse
/// error is the parser's registered diagnostic, located in the source;
/// like the parser, the adapter repeats it on every later pull.
pub struct ParseEvents<'s> {
    parser: Parser<'s>,
    text: &'s str,
    path: String,
    window: Option<Window>,
}

/// One node's subtree out of a whole document: the node is found by its
/// position in document order, and the stream ends with its end.
#[derive(Clone, Copy, Debug)]
struct Window {
    node: u32,
    seen: u32,
    depth: usize,
    done: bool,
}

impl<'s> ParseEvents<'s> {
    pub fn new(parser: Parser<'s>, text: &'s str, path: impl Into<String>) -> Self {
        Self {
            parser,
            text,
            path: path.into(),
            window: None,
        }
    }

    /// Only the subtree of the `node`th node in document order; every
    /// event before it is parsed and dropped, and nothing after it is
    /// read.
    pub(crate) fn windowed(mut self, node: u32) -> Self {
        self.window = Some(Window {
            node,
            seen: 0,
            depth: 0,
            done: false,
        });
        self
    }

    /// Whether the window lets `step` through.
    fn admits(&mut self, step: &Step<NodeStart<'s>, EntryValue<'s>>) -> bool {
        let Some(window) = self.window.as_mut() else {
            return true;
        };
        if window.depth == 0 {
            if let Step::Node(_) = step {
                if window.seen == window.node {
                    window.depth = 1;
                    return true;
                }
                window.seen = window.seen.saturating_add(1);
            }
            return false;
        }
        match step {
            Step::Node(_) => window.depth += 1,
            Step::End => {
                window.depth -= 1;
                window.done = window.depth == 0;
            }
            _ => {}
        }
        true
    }
}

impl<'s> EventSource for ParseEvents<'s> {
    type Node = NodeStart<'s>;
    type Entry = EntryValue<'s>;

    fn next_event(&mut self) -> Pulled<NodeStart<'s>, EntryValue<'s>> {
        loop {
            if self.window.is_some_and(|window| window.done) {
                return Ok(None);
            }
            let event = match self.parser.next_event() {
                Ok(Some(event)) => event,
                Ok(None) => return Ok(None),
                Err(error) => {
                    let record = error
                        .to_diagnostic(self.text)
                        .with_source(self.path.clone());
                    return Err(Box::new(record));
                }
            };
            let step = match event.item {
                Event::Node { ty, name } => Step::Node(NodeStart::new(name, ty.is_some())),
                Event::Entry(entry) => Step::Entry(EntryValue::new(
                    entry.name,
                    entry.ty.is_some(),
                    convert(entry.value),
                )),
                Event::Children => Step::Children,
                Event::End => Step::End,
                // A future event kind carries no binding structure.
                _ => continue,
            };
            if !self.admits(&step) {
                continue;
            }
            return Ok(Some(SourceEvent::new(step, event.span)));
        }
    }
}

fn convert(value: KdlValue<'_>) -> Value {
    match value {
        KdlValue::String(text) => Value::String(text.into_owned()),
        KdlValue::Number(number) => Value::Number(number.text().to_owned()),
        KdlValue::Bool(flag) => Value::Bool(flag),
        _ => Value::Null,
    }
}
