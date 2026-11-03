//! Exercise the KDL writer: build a document whose names and strings
//! carry every character class the writer must handle — quotes,
//! backslashes, newlines and tabs, a non-ASCII letter, a bidi control,
//! a non-breaking space, a name that reads as a number or a keyword, an
//! empty string, an identifier value written bare and one that must be
//! quoted — render it, print the text, then read the text back
//! with the parser and print every event it yields. The harness pins
//! both, so a writer that loses a character or writes text the parser
//! reads differently fails the scenario.

use kdlith_parse::{Event, Parser, Value, WriteDocument, WriteNode, WriteValue, is_bare_string};

fn main() {
    let mut doc = WriteDocument::new();
    doc.comment("written by the writer exercise\nsecond comment line");
    doc.node(
        WriteNode::new("system")
            .arg("ICD10_SE")
            .prop("kind", WriteValue::identifier("classification"))
            .prop("label", WriteValue::identifier("needs quotes"))
            .prop("count", 3u64)
            .prop("active", true)
            .prop("retired", false)
            .child(WriteNode::new("name").arg("sv").arg("Diagnos \"A\" \\ B"))
            .child(WriteNode::new("note").arg("line one\nline two\ttab"))
            .comment("an empty and an odd string follow")
            .child(WriteNode::new("empty").arg(""))
            .child(WriteNode::new("odd").arg("bidi\u{200E}nbsp\u{A0}end")),
    );
    doc.node(
        WriteNode::new("123")
            .prop("true", "x=y")
            .prop("has space", "#z"),
    );
    doc.node(WriteNode::new("leaf").block());
    let text = doc.render();
    print!("{text}");
    println!("--- events");
    for event in Parser::new(&text) {
        match event {
            Ok(spanned) => match spanned.item {
                Event::Node { name, .. } => println!("node {name:?}"),
                Event::Entry(entry) => {
                    let value = match entry.value {
                        Value::String(text) => format!("string {text:?}"),
                        Value::Number(number) => format!("number {}", number.text()),
                        Value::Bool(flag) => format!("bool {flag}"),
                        Value::Null => "null".to_owned(),
                        _ => "other".to_owned(),
                    };
                    match entry.name {
                        Some(name) => println!("prop {name:?} {value}"),
                        None => println!("arg {value}"),
                    }
                }
                Event::Children => println!("children"),
                Event::End => println!("end"),
                _ => println!("other"),
            },
            Err(error) => println!("error {error}"),
        }
    }
    println!("--- bare");
    for word in [
        "ICD10_SE", "123", "-1", ".5", "true", "inf", "a=b", "x y", "", "Ä",
    ] {
        println!("{word:?} {}", is_bare_string(word));
    }
}
