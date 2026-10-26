//! Exercise the generated processor of a schema whose reference type
//! declares the open-world arm: load the committed schema through the
//! generated loader, collect a committed data document as a linked
//! reading through the generated typed reader, and resolve every
//! mention's endpoint reference through the reading — a declared
//! concept resolves to its declaration, and a code of a system whose
//! distribution member holds a listed value answers external at its
//! site, with no declaration and no default value. Every name the
//! exercise spells comes from the generated crate alone.

use std::path::PathBuf;

use openworld_proc::types::{ConceptDef, Mentions};
use openworld_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemVariant, Resolved, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("openworld-exercise: usage: openworld-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("openworld-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let schema = PathBuf::from(suite).join(schema);
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!(
        "schema ok digest={} documents={}",
        definition.digest(),
        definition.documents.len()
    );

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data])
        .unwrap_or_else(|error| {
            eprintln!("openworld-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut mentions: Vec<&Mentions> = Vec::new();
    let mut declarations = 0;
    for item in reading.items() {
        match item {
            TypedItem::Record { value, .. } => mentions.extend(ItemVariant::<Mentions>::variant(value)),
            TypedItem::Declaration { .. } => declarations += 1,
            _ => {}
        }
    }
    println!("read ok mentions={} declarations={declarations}", mentions.len());
    println!("linked failed={}", reading.failed());

    for mention in &mentions {
        let answer = match reading.resolve::<ConceptDef>(&mention.target) {
            Ok(Resolved::Declaration(concept)) => {
                format!("{}.{}", concept.system, concept.code)
            }
            Ok(Resolved::External) => "external".to_owned(),
            Ok(_) => unreachable!("Resolved holds only a declaration or external"),
            Err(error) => format!("error {error}"),
        };
        println!(
            "mention id={} {} -> {answer}",
            mention.id,
            mention.target.written()
        );
    }
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "openworld-exercise: {stage} rejected the input ({} diagnostics)",
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
