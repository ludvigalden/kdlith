//! Exercise the generated processor of a schema whose record and
//! declaration share a node name and an item variant: read a committed
//! data document and print each item under its role-keyed variant,
//! matching the Item enum exhaustively with no wildcard arm, with the
//! name its declarations carry.

use std::path::PathBuf;

use fleet_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("clash-exercise: usage: clash-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        let value = match item {
            TypedItem::Record { value, .. } | TypedItem::Declaration { value, .. } => value,
            _ => return,
        };
        let name = value.name().unwrap_or("-");
        lines.push(match &value {
            Item::HostRecord(host) => format!("host record path={} name={name}", host.path),
            Item::HostDeclaration(kind) => format!("host declaration kind={} name={name}", kind.name),
            Item::Zone(kind) => format!("zone kind={} name={name}", kind.name),
        });
    };
    match reading.read(&[data], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("clash-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("clash-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok items={}", lines.len());
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("clash-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
