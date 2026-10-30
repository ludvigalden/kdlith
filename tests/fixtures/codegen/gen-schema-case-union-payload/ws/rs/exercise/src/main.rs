//! Exercise the generated processor of a schema whose `detail` node
//! dispatches its payload on a `kind` property into a union whose
//! variant types each require `kind`: collect a committed data document
//! as a linked reading through the generated typed reader and print,
//! per holder, the variant its detail decoded as with the `kind` value
//! that variant carries. Every name the exercise spells comes from the
//! generated crate alone.

use std::path::PathBuf;

use case_union_proc::types::{Holder, Input};
use case_union_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("case-union-exercise: usage: case-union-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data])
        .unwrap_or_else(|error| {
            eprintln!("case-union-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut holders: Vec<&Holder> = Vec::new();
    for item in reading.items() {
        if let TypedItem::Record { value, .. } = item {
            holders.extend(ItemVariant::<Holder>::variant(value));
        }
    }
    println!(
        "read ok holders={} diagnostics={}",
        holders.len(),
        reading.diagnostics().len()
    );
    println!("linked failed={}", reading.failed());
    for holder in &holders {
        match &holder.detail {
            Input::A(a) => println!(
                "holder {} variant=a kind={} note={}",
                holder.name,
                a.kind,
                a.note.as_deref().unwrap_or("-")
            ),
            Input::B(b) => println!(
                "holder {} variant=b kind={} size={}",
                holder.name, b.kind, b.size
            ),
        }
    }
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
}

fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    for record in records {
        eprintln!("case-union-exercise: {stage} rejected: {}", record.code);
    }
    std::process::exit(1);
}
