//! Exercise record-level context capture through a generated processor:
//! read committed data whose root control sets a slot for its block's
//! children only, through the generated typed reader unarmed and asked
//! to capture the slot, and print each document, record, and
//! declaration with its declaring node and the slot as that item binds
//! under it, beside the value the record's own context-sourced field
//! bound. Every name the exercise spells comes from the generated crate
//! alone.

use std::path::PathBuf;

use shelf_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, Value, load, reader};

/// Captured slots as `name=value`, a string slot as its text and an
/// unset one as `unset`; a macro, so each item's captured value is read
/// where its type is known without the exercise naming it.
macro_rules! slots {
    ($context:expr) => {
        $context
            .iter()
            .map(|(slot, value)| {
                let shown = match value.as_ref().map(|captured| &captured.value) {
                    None => "unset".to_owned(),
                    Some(Value::String(text)) => text.clone(),
                    Some(other) => format!("{other:?}"),
                };
                format!("{slot}={shown}")
            })
            .collect::<Vec<_>>()
            .join(",")
    };
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("capture-record-exercise: usage: capture-record-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(&[schema.as_path()], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let capturing = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .capturing(&["region"]);
    for (label, reading) in [("plain", &plain), ("capturing", &capturing)] {
        let mut lines: Vec<String> = Vec::new();
        let mut sink = |item: TypedItem<Item>| match item {
            TypedItem::Document { path, context, .. } => {
                lines.push(format!("document {path} context=[{}]", slots!(context)));
            }
            TypedItem::Record {
                node_name,
                value: Item::Fruit(fruit) | Item::Pick(fruit),
                context,
                ..
            } => {
                let bound = fruit.region.as_ref().map_or("unset", |region| region.as_str());
                lines.push(format!(
                    "record {node_name} id={} field={bound} context=[{}]",
                    fruit.id,
                    slots!(context)
                ));
            }
            TypedItem::Declaration {
                node_name,
                value: Item::Bin(bin),
                context,
                ..
            } => {
                lines.push(format!(
                    "declaration {node_name} name={} context=[{}]",
                    bin.name,
                    slots!(context)
                ));
            }
            _ => {}
        };
        match reading.read(&[data.as_path()], &mut sink) {
            Ok(failed) => println!("read {label} failed={failed}"),
            Err(error) => {
                eprintln!("capture-record-exercise: data read failed: {error}");
                std::process::exit(1);
            }
        }
        for line in &lines {
            println!("{line}");
        }
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("capture-record-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
