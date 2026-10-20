//! Exercise the generated processor of a schema whose dependent value
//! binding names a known codec no processor installs: collect committed
//! data as readings linked under complete and local scope through the
//! generated reader, and print each pending payload with what the link
//! stage made of it — a payload that does not fit the member type of the
//! declaration its reference resolved to fails `KDLITH-REF005`, and one
//! whose reference resolves to nothing fails `KDLITH-REF001` alone, its
//! payload never validated. Every kdlith name the exercise spells comes
//! from the generated crate alone.

use std::path::PathBuf;

use put_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, Reading, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("pending-payload-exercise: usage: pending-payload-exercise <schema> <data>");
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
                eprintln!("pending-payload-exercise: reading {} broke: {error}", data.display());
                std::process::exit(1);
            });
        print_reading(label, &reading);
    }
}

fn print_reading(label: &str, reading: &Reading<Item>) {
    let pending: Vec<String> = reading
        .items()
        .iter()
        .filter_map(|item| match item {
            TypedItem::PendingBinding(binding) => Some(format!(
                "pending against={} raw={}",
                binding.against.written,
                serde_json::to_string(&binding.raw).unwrap_or_default()
            )),
            _ => None,
        })
        .collect();
    println!(
        "linked {label} failed={} pending={} diagnostics={}",
        reading.failed(),
        pending.len(),
        reading.diagnostics().len()
    );
    for line in &pending {
        println!("{line}");
    }
    for record in reading.diagnostics() {
        println!(
            "diag {} {}:{} {}",
            record.code,
            record.line.unwrap_or(0),
            record.col.unwrap_or(0),
            record.message.as_deref().unwrap_or("")
        );
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("pending-payload-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
