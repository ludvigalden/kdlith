//! Exercise the generated processor of a schema whose part references
//! are sibling-qualified: collect the committed data document as a
//! reading linked under complete scope through the generated typed
//! reader, and print every render with the qualified identity its
//! references resolved to — the work reference through the reading's
//! qualified-name accessor, each bare part spelling the same way — so
//! the same part slug under two works resolves inside its own work
//! and the accessor returns the fully qualified identity while
//! `Ref::written` still carries the bare spelling.

use std::path::PathBuf;

use part_proc::types::Render;
use part_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, Reading, Ref, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("part-exercise: usage: part-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("part-exercise: KDLITH_FX_SUITE is not set; run under the harness");
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
            eprintln!("part-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let renders: Vec<&Render> = reading
        .items()
        .iter()
        .filter_map(|item| match item {
            TypedItem::Record { value, .. } => ItemVariant::<Render>::variant(value),
            _ => None,
        })
        .collect();
    println!(
        "read failed={} renders={} diagnostics={}",
        reading.failed(),
        renders.len(),
        reading.diagnostics().len()
    );
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
    for render in renders {
        let work = qualified(&reading, &render.work);
        let parts: Vec<String> = render
            .parts
            .iter()
            .map(|part| format!("{}->{}", part.written(), qualified(&reading, part)))
            .collect();
        println!(
            "render {} work={}->{} parts=[{}]",
            render.id,
            render.work.written(),
            work,
            parts.join(",")
        );
    }
}

/// A reference's fully qualified identity, as the linked reading
/// resolved it; `-` when it holds none.
fn qualified<T, V>(reading: &Reading<T>, reference: &Ref<V>) -> String {
    reading
        .qualified_name(reference)
        .map_or("-".to_owned(), |segments| segments.join("."))
}

fn reject(what: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("part-exercise: {what} rejected:");
    for record in records {
        eprintln!(
            "  {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        );
    }
    std::process::exit(1);
}
