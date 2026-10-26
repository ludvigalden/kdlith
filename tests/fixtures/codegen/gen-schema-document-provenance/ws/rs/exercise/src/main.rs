//! Exercise the processor generated from a schema declaring profiles:
//! read committed data through the generated typed reader, printing the
//! profiles each document's entry invocation selected when provenance
//! was asked for — named, match-selected from the source path, and the
//! members of a selected set — and none when it was not, beside the
//! refusal of a provenance the schema cannot declare: an unknown
//! profile named in the reading's options fails before any document is
//! read.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, ReadOptions, TypedItem};
use provenance_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, stock, retail] = args.as_slice() else {
        eprintln!("provenance-exercise: usage: provenance-exercise <schema> <stock-data> <retail-data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    read("plain", &plain, &[stock.as_path()]);

    // A named profile reaches every document of the reading, the
    // entry's imports among them.
    let named = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .with_profile("retail")
        .provenance();
    read("named", &named, &[retail.as_path()]);

    // With no name, the entry's source path selects: the stock entry
    // satisfies the default profile's path-prefix rule.
    let inferred = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .provenance();
    read("inferred", &inferred, &[stock.as_path()]);

    // A listed set the retail entry's path satisfies twice selects both
    // members, in listed order.
    let options = ReadOptions::new().with_profiles(["retail", "plain"]);
    let listed = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .with_options(options)
        .provenance();
    read("listed", &listed, &[retail.as_path()]);

    // An unknown profile named in the options the opt-in surfaces fails
    // the reading before any document is read.
    let unknown = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .with_profile("nope")
        .provenance();
    let mut items = 0;
    match unknown.read(&[stock.as_path()], &mut |_| items += 1) {
        Ok(failed) => println!("unknown-profile read failed={failed} items={items}"),
        Err(error) => println!("unknown-profile error={error} items={items}"),
    }
}

/// One reading under `label`, printing each document with the profiles
/// it declared when the reader asked for them.
fn read(label: &str, reading: &kdlith_engine::TypedReader<'_, Item>, entries: &[&std::path::Path]) {
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Document { path, profiles, .. } = item {
            let names = profiles.join(",");
            lines.push(format!("document {path} profiles=[{names}]"));
        }
    };
    match reading.read(entries, &mut sink) {
        Ok(failed) => println!("read {label} failed={failed}"),
        Err(error) => {
            eprintln!("provenance-exercise: {label} read failed: {error}");
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
        "provenance-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
