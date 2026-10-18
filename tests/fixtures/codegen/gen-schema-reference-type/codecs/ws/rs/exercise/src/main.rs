//! Exercise the generated processor of a schema declaring a pattern
//! codec: load the committed codec schema through the generated loader,
//! read the committed data document through the generated typed reader,
//! and print the decoded output value each record's codec-typed field
//! carries.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, ItemVariant, LogicalType, TypedItem};
use timing_proc::types::{Event, Stamp};
use timing_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("codec-exercise: usage: codec-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    // Schema and data are the committed suite files themselves, never
    // copies: their paths are relative to the suite root the harness
    // exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("codec-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let suite = PathBuf::from(suite);
    let extensions = Extensions::standard();

    let definition = load(&[suite.join(schema).as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    println!("codec {} output={}", Stamp::expression(), u64::expression());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(event) = ItemVariant::<Event>::variant(&value)
        {
            lines.push(format!("event stamp={}", event.stamp.0));
        }
    };
    let path = suite.join(data);
    match reading.read(&[path.as_path()], &mut sink) {
        Ok(errors) => println!("read errors={errors} records={}", lines.len()),
        Err(error) => {
            eprintln!("codec-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "codec-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    for record in records {
        eprintln!(
            "  {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        );
    }
    std::process::exit(1);
}
