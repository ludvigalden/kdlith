//! Exercise the generated processor of a schema whose set nodes bind
//! path-typed values: load the committed schema through the generated
//! loader, collect a committed data document as a linked reading
//! through the generated typed reader, and report every typed-binding
//! item the value-from-path nodes leave at their stream position —
//! each resolved one naming its subject type and path — beside the
//! amendment record. Every name the exercise spells comes from the
//! generated crate alone.

use std::path::PathBuf;

use path_value_proc::types::Amend;
use path_value_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("path-value-exercise: usage: path-value-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("path-value-exercise: KDLITH_FX_SUITE is not set; run under the harness");
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
            eprintln!("path-value-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut amends: Vec<&Amend> = Vec::new();
    let mut pending = 0;
    for item in reading.items() {
        match item {
            TypedItem::Record { value, .. } => amends.extend(ItemVariant::<Amend>::variant(value)),
            TypedItem::PendingBinding(binding) => {
                pending += 1;
                println!(
                    "typed-binding {} {}",
                    binding.against.domain, binding.against.written
                );
            }
            _ => {}
        }
    }
    println!("read ok amends={} pending={pending}", amends.len());
    println!("linked failed={}", reading.failed());
    for amend in &amends {
        println!("amend {} changes={}", amend.id, amend.changes.len());
    }
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
}

fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    for record in records {
        eprintln!("path-value-exercise: {stage} rejected: {}", record.code);
    }
    std::process::exit(1);
}
