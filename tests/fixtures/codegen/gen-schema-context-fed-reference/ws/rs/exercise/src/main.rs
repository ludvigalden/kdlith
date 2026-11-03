//! Exercise the generated processor of a schema whose reference-typed
//! fields take values from a context slot: collect the committed data
//! document as a reading linked under complete scope through the
//! generated typed reader, and print every book with the references its
//! fields hold — a context-fed one, a one-element list fed one value, a
//! contextual one carrying the context it was bound under — and what
//! each resolves to through the reading. No book is dropped for a
//! context-fed value. It then builds a document through the KDL writer
//! the generated crate re-exports, renders it, and reads the text back
//! with the parser, printing every event, so a writer that loses a
//! character or writes text the parser reads differently fails.

use std::path::PathBuf;

use shelf_proc::types::{Book, Shelf, Spot};
use kdlith_parse::{Event, Parser, Value, is_bare_string};
use shelf_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, Reading, Ref, Resolved, TypedItem,
    WriteDocument, WriteNode, WriteValue, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("shelf-exercise: usage: shelf-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("shelf-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let suite = PathBuf::from(suite);
    let definition = load(
        &[suite.join(schema).as_path()],
        &Extensions::standard(),
        CheckScope::Complete,
    )
    .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[suite.join(data).as_path()])
        .unwrap_or_else(|error| {
            eprintln!("shelf-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let books: Vec<&Book> = reading
        .items()
        .iter()
        .filter_map(|item| match item {
            TypedItem::Record { value, .. } => ItemVariant::<Book>::variant(value),
            _ => None,
        })
        .collect();
    println!(
        "read failed={} books={} diagnostics={}",
        reading.failed(),
        books.len(),
        reading.diagnostics().len()
    );
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
    for book in books {
        let shelf = book
            .shelf
            .as_ref()
            .map_or("-".to_owned(), |shelf| shelved(&reading, shelf));
        let tags: Vec<String> = book.tags.iter().map(|tag| shelved(&reading, tag)).collect();
        let placed = book.placed.as_ref().map_or("-".to_owned(), |placed| {
            let to = match reading.resolve::<Spot>(placed) {
                Ok(Resolved::Declaration(spot)) => format!("{}.{}", spot.room, spot.id),
                Ok(_) => "external".to_owned(),
                Err(error) => format!("error {error}"),
            };
            format!(
                "{} context={} -> {to}",
                placed.written(),
                placed.context().unwrap_or("-")
            )
        });
        println!(
            "book {} shelf={shelf} tags=[{}] placed={placed}",
            book.title,
            tags.join(",")
        );
    }
    writer_roundtrip();
}

/// Render a document through the writer and read it back.
fn writer_roundtrip() {
    println!("--- writer");
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

/// A shelf reference as written, and the declaration it resolved to.
fn shelved<T: ItemVariant<Shelf>>(reading: &Reading<T>, shelf: &Ref<Shelf>) -> String {
    match reading.resolve::<Shelf>(shelf) {
        Ok(Resolved::Declaration(declared)) => format!("{}->{}", shelf.written(), declared.id),
        Ok(_) => format!("{} external", shelf.written()),
        Err(error) => format!("{} error {error}", shelf.written()),
    }
}

fn reject(what: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("shelf-exercise: {what} rejected:");
    for record in records {
        eprintln!(
            "  {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        );
    }
    std::process::exit(1);
}
