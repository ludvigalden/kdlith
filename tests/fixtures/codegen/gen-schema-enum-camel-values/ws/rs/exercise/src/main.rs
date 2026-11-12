//! Exercise the generated schema processor's enum spellings: load the
//! committed schema through the generated loader, read a committed data
//! document through the generated typed reader, and print every value
//! twice — as its Rust variant (Debug, the UpperCamelCase identifier)
//! and as its member string (Display, the exact KDL value) — proving
//! the underscore, kebab, and mixed-case spellings round-trip unchanged.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use camelvalues_proc::types::Entry;
use camelvalues_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("schema-exercise: usage: schema-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut entries: Vec<Entry> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| {
        if let TypedItem::Record {
            value: Item::Entry(entry),
            ..
        } = item
        {
            entries.push(entry);
        }
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("schema-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("schema-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok entries={}", entries.len());
    for entry in &entries {
        println!("outcome {:?} spells {}", entry.outcome, entry.outcome);
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "schema-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
