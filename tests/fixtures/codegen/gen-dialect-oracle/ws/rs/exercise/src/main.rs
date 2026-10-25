//! Exercise the generated oracle processor end to end: compile the
//! embedded canonical seed, load a committed schema through the
//! generated loader, read the schema's own items through the
//! generated typed reader, and print the doc comment the generated
//! types carry on the canonical MergePolicy vocabulary.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use oracle_proc::{Item, load_schema, reader, seed};

fn main() {
    let mut args = std::env::args().skip(1).map(PathBuf::from);
    let (Some(schema), Some(types)) = (args.next(), args.next()) else {
        eprintln!("oracle-exercise: usage: oracle-exercise <schema> <generated-types>");
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

    let generated = std::fs::read_to_string(&types).unwrap_or_else(|error| {
        eprintln!("oracle-exercise: cannot read {}: {error}", types.display());
        std::process::exit(2);
    });
    let doc = merge_policy_doc(&generated).unwrap_or_else(|| {
        eprintln!("oracle-exercise: the generated types carry no MergePolicy doc");
        std::process::exit(1);
    });
    println!("merge policy doc={doc}");
}

/// The `///` doc lines the generated types carry directly above the
/// `MergePolicy` enum, joined as one sentence.
fn merge_policy_doc(generated: &str) -> Option<String> {
    let lines: Vec<&str> = generated.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("pub enum MergePolicy"))?;
    let mut start = at;
    while start > 0 && lines[start - 1].trim_start().starts_with("///") {
        start -= 1;
    }
    if start == at {
        return None;
    }
    let doc: Vec<String> = lines[start..at]
        .iter()
        .map(|line| line.trim_start().trim_start_matches("///").trim().to_owned())
        .collect();
    Some(doc.join(" "))
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "oracle-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
