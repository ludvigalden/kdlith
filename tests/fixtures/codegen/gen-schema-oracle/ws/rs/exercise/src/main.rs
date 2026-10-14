//! Exercise the generated schema processor end to end: load the
//! committed schema through the generated loader, read committed data
//! documents through the generated typed reader, and require the
//! rejection a document the schema forbids produces.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use shelf_proc::types::{Entry, Group};
use shelf_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, invalid] = args.as_slice() else {
        eprintln!("schema-exercise: usage: schema-exercise <schema> <data> <invalid-data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut entries: Vec<Entry> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| match item {
        TypedItem::Record {
            value: Item::Entry(entry),
            ..
        } => entries.push(entry),
        TypedItem::Declaration {
            value: Item::Group(group),
            ..
        } => groups.push(group),
        _ => {}
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("schema-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("schema-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok entries={} groups={}", entries.len(), groups.len());
    for entry in &entries {
        println!(
            "entry id={} tier={} locale={}",
            entry.id,
            entry.tier.member(),
            entry.locale.as_ref().map_or("-", |locale| locale.as_str())
        );
    }
    for group in &groups {
        println!("group name={}", group.name);
    }

    let mut codes: Vec<String> = Vec::new();
    let mut invalid_sink = |item: TypedItem<Item>| {
        if let TypedItem::Diagnostic(record) = item {
            codes.push(record.code.clone());
        }
    };
    let rejected = match reading.read(&[invalid], &mut invalid_sink) {
        Ok(had_errors) => had_errors,
        Err(error) => {
            eprintln!("schema-exercise: invalid read failed: {error}");
            std::process::exit(1);
        }
    };
    if rejected && codes.iter().any(|code| code == "KDLITH-BND008") {
        println!("rejected KDLITH-BND008");
    } else {
        eprintln!(
            "schema-exercise: the invalid document was not rejected as declared \
             (had_errors={rejected}, codes={codes:?})"
        );
        std::process::exit(1);
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "schema-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
