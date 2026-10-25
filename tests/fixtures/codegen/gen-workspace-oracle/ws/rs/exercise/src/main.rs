//! Exercise a generated processor whose manifest was written with
//! `--kdlith-workspace`: the consumer needs only the generated crate,
//! and the compiled crate resolves its kdlith dependencies and lints
//! from the scratch workspace's root manifest.

use std::path::PathBuf;

use parts_proc::{CheckScope, Extensions, Item, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("parts-exercise: usage: parts-exercise <schema> <data>");
        std::process::exit(2);
    };
    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok");
    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record {
            value: Item::Part(part),
            ..
        } = item
        {
            lines.push(format!(
                "part id={} count={}",
                part.id.as_str(),
                part.count.map_or("none".to_owned(), |count| count.to_string())
            ));
        }
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("parts-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[parts_proc::DiagnosticRecord]) -> ! {
    eprintln!("parts-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
