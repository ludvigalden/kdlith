//! Exercise the generated processor of a schema with an import-projected
//! context slot: read committed data through the generated typed reader
//! asked to capture the slot, and print each document's captured value
//! as its first node binds under it, then each stamp record with its
//! provenance. A reader capturing a slot the schema does not declare
//! fails before any document is read. Every name the exercise spells
//! comes from the generated crate alone.

use std::path::PathBuf;

use imports_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("capture-exercise: usage: capture-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    // Schema and data are the committed suite files themselves, read from
    // the suite root so each document's shown path is the one `vet data`
    // reports there.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("capture-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    if let Err(error) = std::env::set_current_dir(&suite) {
        fail("entering the suite root", &error);
    }

    let definition = load(&[schema.as_path()], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let capturing = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .capturing(&["locale"]);
    for (label, reading) in [("plain", &plain), ("capturing", &capturing)] {
        let mut lines: Vec<String> = Vec::new();
        let mut sink = |item: TypedItem<Item>| match item {
            TypedItem::Document { path, context, .. } => {
                let slots: Vec<String> = context
                    .iter()
                    .map(|(slot, value)| {
                        let value = value
                            .as_ref()
                            .map_or("unset".to_owned(), |captured| shown(&captured.value));
                        format!("{slot}={value}")
                    })
                    .collect();
                lines.push(format!("document {path} context=[{}]", slots.join(",")));
            }
            TypedItem::Record {
                value: Item::Stamp(stamp),
                site,
                origin,
                ..
            } => {
                let line = site.map_or("?".to_owned(), |site| site.span.start.line.to_string());
                lines.push(format!(
                    "stamp at={} site={}:{line}",
                    stamp.at.as_str(),
                    origin.path
                ));
            }
            _ => {}
        };
        match reading.read(&[data.as_path()], &mut sink) {
            Ok(failed) => println!("read {label} failed={failed}"),
            Err(error) => fail("data read", &error),
        }
        for line in &lines {
            println!("{line}");
        }
    }

    // An unknown slot fails the reading before any document is read.
    let unknown = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .capturing(&["profile"]);
    let mut items = 0;
    match unknown.read(&[data.as_path()], &mut |_| items += 1) {
        Ok(failed) => println!("unknown-slot read failed={failed} items={items}"),
        Err(error) => println!("unknown-slot error={error} items={items}"),
    }
}

/// A captured value as the context view spells a string, or its debug
/// form for any other kind.
fn shown(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    match debug.strip_prefix("String(\"").and_then(|rest| rest.strip_suffix("\")")) {
        Some(text) => text.to_owned(),
        None => debug,
    }
}

fn fail(stage: &str, error: &dyn std::fmt::Display) -> ! {
    eprintln!("capture-exercise: {stage} failed: {error}");
    std::process::exit(1);
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("capture-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
