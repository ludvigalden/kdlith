//! Exercise the generated processor of a schema whose mapping id is
//! ordinal-derived and whose pack code is parent-derived: load the
//! committed schema through the generated loader, collect a committed
//! data document as linked readings under complete and local scope
//! through the generated typed reader, and print every declaration with
//! the derived value the typed record and composed identity carry — the
//! omitted mapping ids bind as source-dot-ordinal, the explicit id wins,
//! and every pack's code is its enclosing concept's. Every kdlith name
//! the exercise spells comes from the generated crate alone.

use std::path::PathBuf;

use derived_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("derived-id-exercise: usage: derived-id-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(&[schema.as_path()], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    for (label, scope) in [("complete", CheckScope::Complete), ("local", CheckScope::Local)] {
        let reading = reader(&definition)
            .unwrap_or_else(|records| reject("reader", &records))
            .link(scope)
            .collect(&[data.as_path()])
            .unwrap_or_else(|error| {
                eprintln!("derived-id-exercise: reading {} broke: {error}", data.display());
                std::process::exit(1);
            });
        println!(
            "linked {label} failed={} declarations={}",
            reading.failed(),
            reading
                .items()
                .iter()
                .filter(|item| matches!(item, TypedItem::Declaration { .. }))
                .count()
        );
        for item in reading.items() {
            let TypedItem::Declaration { identity, value, .. } = item else {
                continue;
            };
            let name = identity.name.0.join(".");
            match value {
                Item::Concept(concept) => {
                    println!("concept {name} code={}", concept.code);
                }
                Item::Mapping(mapping) => {
                    println!("mapping {name} id={} source={}", mapping.id, mapping.source);
                }
                Item::Pack(pack) => {
                    println!("pack {name} code={} note={}", pack.code, pack.note);
                }
            }
        }
        for record in reading.diagnostics() {
            println!("diag {} {}", record.code, record.message.as_deref().unwrap_or(""));
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
    eprintln!("derived-id-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
