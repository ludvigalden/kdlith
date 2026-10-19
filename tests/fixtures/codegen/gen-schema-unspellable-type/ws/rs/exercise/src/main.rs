//! Exercise the generated processor of a schema declaring a type named
//! `Option`: the generated crate spells the prelude's option by path, so
//! the declared type and an optional field coexist, and both read.

use std::path::PathBuf;

use shadow_proc::types::{Entry, Option as Declared};
use shadow_proc::{CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("shadow-exercise: usage: shadow-exercise <schema> <data>");
        std::process::exit(2);
    };
    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Declaration { value, .. } | TypedItem::Record { value, .. } => {
            if let Some(option) = ItemVariant::<Declared>::variant(&value) {
                lines.push(format!("option name={}", option.name));
            }
            if let Some(entry) = ItemVariant::<Entry>::variant(&value) {
                lines.push(format!("entry pick={}", entry.pick.as_deref().unwrap_or("absent")));
            }
        }
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("shadow-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("shadow-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
