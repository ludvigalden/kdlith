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
}

impl<'s> ParseEvents<'s> {
    pub fn new(parser: Parser<'s>, text: &'s str, path: impl Into<String>) -> Self {
        Self {
            parser,
            text,
            path: path.into(),
        }
    }
}

impl<'s> EventSource for ParseEvents<'s> {
    type Node = NodeStart<'s>;
    type Entry = EntryValue<'s>;

    fn next_event(&mut self) -> Pulled<NodeStart<'s>, EntryValue<'s>> {
        loop {
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
