//! Exercise the processor generated from a schema declaring no
//! profiles: the reading itself is sound, but a typed reader asked for
//! provenance fails before any document is read, because no document
//! can declare a profile the schema does not declare.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use plain_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("provenance-unprofiled-exercise: usage: provenance-unprofiled-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut documents = 0;
    let mut sink = |item: TypedItem<Item>| {
        if matches!(item, TypedItem::Document { .. }) {
            documents += 1;
        }
    };
    match plain.read(&[data.as_path()], &mut sink) {
        Ok(failed) => println!("read plain failed={failed} documents={documents}"),
        Err(error) => {
            eprintln!("provenance-unprofiled-exercise: plain read failed: {error}");
            std::process::exit(1);
        }
    }

    // Provenance on a definition that declares no profiles fails the
    // reading before any document is read.
    let asking = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .provenance();
    let mut items = 0;
    match asking.read(&[data.as_path()], &mut |_| items += 1) {
        Ok(failed) => println!("no-profiles read failed={failed} items={items}"),
        Err(error) => println!("no-profiles error={error} items={items}"),
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "provenance-unprofiled-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
