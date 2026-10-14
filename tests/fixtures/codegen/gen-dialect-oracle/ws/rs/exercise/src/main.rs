//! Exercise the generated oracle processor end to end: compile the
//! embedded canonical seed, load a committed schema through the
//! generated loader, and read the schema's own items through the
//! generated typed reader.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use oracle_proc::{Item, load_schema, reader, seed};

fn main() {
    let Some(schema) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("oracle-exercise: usage: oracle-exercise <schema>");
        std::process::exit(2);
    };
    let schema = schema.as_path();
    let extensions = Extensions::standard();

    let dialect = seed(&extensions).unwrap_or_else(|records| reject("seed", &records));
    println!("seed ok");

    let definition = load_schema(&dialect, &[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&dialect).unwrap_or_else(|records| reject("reader", &records));
    let mut records = 0usize;
    let mut declarations = 0usize;
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Record { .. } => records += 1,
        TypedItem::Declaration { .. } => declarations += 1,
        _ => {}
    };
    match reading.read(&[schema], &mut sink) {
        Ok(had_errors) if had_errors => {
            eprintln!("oracle-exercise: the reading reported error diagnostics");
            std::process::exit(1);
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("oracle-exercise: read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok records={records} declarations={declarations}");
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "oracle-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
