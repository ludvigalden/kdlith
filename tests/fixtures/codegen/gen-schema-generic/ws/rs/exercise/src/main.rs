//! Exercise the generated processor of a schema instantiating a generic
//! declaration: the monomorphized instance reads under its mangled Rust
//! name while its shape and expression spell the instantiation, and the
//! case-dispatched records read through the instance's handler.

use std::path::PathBuf;

use couples_proc::types::PairStringMode;
use couples_proc::{CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader};
use kdlith_engine::{LogicalType, Shaped, TypeShape};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("generic-exercise: usage: generic-exercise <schema> <data>");
        std::process::exit(2);
    };
    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    if let TypeShape::Object { name, fields } = PairStringMode::SHAPE {
        let fields: Vec<String> = fields
            .iter()
            .map(|field| format!("{}: {}", field.name, (field.expression)()))
            .collect();
        println!("shape {name} fields=[{}]", fields.join(", "));
    }
    println!(
        "expression {} optional={}",
        PairStringMode::expression(),
        <Option<PairStringMode> as LogicalType>::expression()
    );

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    println!("reader ok");
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Record { value, .. } => {
            if let Some(pair) = ItemVariant::<PairStringMode>::variant(&value) {
                let json = serde_json::to_string(pair).unwrap_or_default();
                lines.push(format!("pair left={} right={:?} json={json}", pair.left, pair.right));
            }
        }
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("generic-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("generic-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
