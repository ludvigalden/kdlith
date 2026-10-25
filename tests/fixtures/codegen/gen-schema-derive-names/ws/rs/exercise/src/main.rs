//! Exercise the generated processor of a schema whose types are named
//! after derived traits: read a committed data document through the
//! generated reader and print each item, comparing and hashing the
//! derive-named values so every derive the macros spell is used.

use std::collections::HashSet;
use std::path::PathBuf;

use derive_proc::types::{Clone as Mode, Hash as Digest, PartialEq as Value};
use derive_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("derive-exercise: usage: derive-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut digests: HashSet<Digest> = HashSet::new();
    let mut sink = |item: TypedItem<Item>| {
        let value = match item {
            TypedItem::Record { value, .. } | TypedItem::Declaration { value, .. } => value,
            _ => return,
        };
        lines.push(match &value {
            Item::Entry(entry) => {
                digests.insert(entry.hash.clone());
                let value = match &entry.value {
                    Value::Number(number) => format!("number:{number}"),
                    Value::Text(text) => format!("text:{text}"),
                };
                format!(
                    "entry id={} hash={} deep={} value={value}",
                    entry.id,
                    entry.hash,
                    entry.mode == Mode::Deep
                )
            }
            Item::Default(default) => format!("default id={}", default.id),
            Item::Note(note) => {
                digests.insert(note.hash.clone());
                format!("note text={} hash={}", note.text, note.hash)
            }
        });
    };
    match reading.read(&[data], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("derive-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("derive-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok items={} digests={}", lines.len(), digests.len());
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("derive-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
