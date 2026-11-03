//! Exercise the generated processor of a schema whose reference-typed
//! fields take values from a context slot: collect the committed data
//! document as a reading linked under complete scope through the
//! generated typed reader, and print every book with the references its
//! fields hold — a context-fed one, a one-element list fed one value, a
//! contextual one carrying the context it was bound under — and what
//! each resolves to through the reading. No book is dropped for a
//! context-fed value. Every name the exercise spells comes from the
//! generated crate alone.

use std::path::PathBuf;

use shelf_proc::types::{Book, Shelf, Spot};
use shelf_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, Reading, Ref, Resolved, TypedItem, load,
    reader,
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
