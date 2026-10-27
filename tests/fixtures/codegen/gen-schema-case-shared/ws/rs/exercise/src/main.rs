//! Exercise the generated processor of a schema whose figure node adds
//! per-case bindings: load the committed schema through the generated
//! loader, collect a committed data document holding all three figure
//! kinds as a linked reading through the generated typed reader, and
//! report which case surface each figure bound — the funnel's depth and
//! sections, the bar's note and segments, and the scatter figure the
//! common fields alone. Every name the exercise spells comes from the
//! generated crate alone.

use std::path::PathBuf;

use caseshared_proc::types::Figure;

use caseshared_proc::{CheckScope, DiagnosticRecord, Extensions, ItemVariant, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("caseshared-exercise: usage: caseshared-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("caseshared-exercise: KDLITH_FX_SUITE is not set; run under the harness");
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
            eprintln!("caseshared-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut figures: Vec<&Figure> = Vec::new();
    for item in reading.items() {
        if let TypedItem::Record { value, .. } = item {
            figures.extend(ItemVariant::<Figure>::variant(value));
        }
    }
    println!("read ok figures={}", figures.len());
    println!("linked failed={}", reading.failed());
    for figure in &figures {
        let depth = figure.depth.map_or("-".to_owned(), |depth| depth.to_string());
        let note = figure.note.as_deref().unwrap_or("-");
        let sections = figure.sections.as_ref().map_or(0, |sections| sections.iter().count());
        let segments = figure.segments.as_ref().map_or(0, Vec::len);
        println!(
            "figure {} depth={} sections={} note={} segments={}",
            figure.id, depth, sections, note, segments
        );
    }
    for record in reading.diagnostics() {
        println!("diag {record:?}");
    }
}

fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    for record in records {
        eprintln!("caseshared-exercise: {stage} rejected: {}", record.code);
    }
    std::process::exit(1);
}
