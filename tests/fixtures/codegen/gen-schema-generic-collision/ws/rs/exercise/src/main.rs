//! Exercise the generated processor of a schema instantiating one generic
//! declaration with two argument sets: each instance reads under its own
//! mangled Rust name with its own shape, and the records each case node
//! dispatches read through that instance's handler.

use std::path::PathBuf;

use kdlith_engine::{LogicalType, Shaped, TypeShape};
use pairs_proc::types::{PairSlugMode, PairStringMode};
use pairs_proc::{CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("generic-collision-exercise: usage: generic-collision-exercise <schema> <data>");
        std::process::exit(2);
    };
    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    describe::<PairStringMode>();
    describe::<PairSlugMode>();

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    println!("reader ok");
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Record { value, .. } => {
            if let Some(pair) = ItemVariant::<PairStringMode>::variant(&value) {
                let json = serde_json::to_string(pair).unwrap_or_default();
                lines.push(format!("string-pair left={} right={:?} json={json}", pair.left, pair.right));
            } else if let Some(pair) = ItemVariant::<PairSlugMode>::variant(&value) {
                let json = serde_json::to_string(pair).unwrap_or_default();
                lines.push(format!("slug-pair left={} right={:?} json={json}", pair.left.as_str(), pair.right));
            } else {
                lines.push("other".to_owned());
            }
        }
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("generic-collision-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Print an instance's shape name, its substituted fields, and its
/// expression alone and under `optional`.
fn describe<T: Shaped + LogicalType>() {
    if let TypeShape::Object { name, fields } = T::SHAPE {
        let fields: Vec<String> = fields
            .iter()
            .map(|field| format!("{}: {}", field.name, (field.expression)()))
            .collect();
        println!("shape {name} fields=[{}]", fields.join(", "));
    }
    println!(
        "expression {} optional={}",
        T::expression(),
        <Option<T> as LogicalType>::expression()
    );
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("generic-collision-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
