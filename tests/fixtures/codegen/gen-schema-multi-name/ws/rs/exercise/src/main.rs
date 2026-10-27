//! Exercise the generated processor of a schema whose one node
//! declaration names several spellings: load the committed schema
//! through the generated loader, collect a committed data document
//! using every spelling as a linked reading through the generated typed
//! reader, and report each bound record under the spelling that read
//! it — the one Item variant matching all spellings and the variant
//! each spelling keys. Every name the exercise spells comes from the
//! generated crate alone.

use std::path::PathBuf;

use multiname_proc::types::Designation;

use multiname_proc::{CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("multiname-exercise: usage: multiname-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("multiname-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let schema = PathBuf::from(suite).join(schema);
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!(
        "schema ok digest={} documents={}",
        definition.digest(),
        definition.documents.len()
    );

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data])
        .unwrap_or_else(|error| {
            eprintln!("multiname-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut designations: Vec<(&str, &Designation)> = Vec::new();
    for item in reading.items() {
        if let TypedItem::Record { value, .. } = item {
            let Some(record) = ItemVariant::<Designation>::variant(value) else {
                continue;
            };
            let spelling = match value {
                Item::Heading(_) => "heading",
                Item::Label(_) => "label",
                Item::Term(_) => "term",
                Item::Alias(_) => "alias",
                Item::Title(_) => "title",
                Item::Name(_) => "name",
            };
            designations.push((spelling, record));
        }
    }
    println!("read ok designations={}", designations.len());
    println!("linked failed={}", reading.failed());
    for (spelling, record) in &designations {
        let weight = record.weight.map_or("-".to_owned(), |w| w.to_string());
        println!("designation {spelling} value={} weight={weight}", record.value);
    }
    for record in reading.diagnostics() {
        println!("diag {record:?}");
    }
}

fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    for record in records {
        eprintln!("multiname-exercise: {stage} rejected: {}", record.code);
    }
    std::process::exit(1);
}
