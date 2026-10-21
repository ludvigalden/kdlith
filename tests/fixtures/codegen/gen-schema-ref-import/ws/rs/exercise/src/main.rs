//! Exercise the generated processor of an import-carrying schema whose
//! record type holds a declared reference type: load the committed
//! schema through the generated loader, collect a committed data
//! document as a linked reading through the generated typed reader, and
//! resolve every entry's group reference through the reading. Every name
//! the exercise spells comes from the generated crate alone.

use std::path::PathBuf;

use catalog_proc::types::{Entry, Group};
use catalog_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, LinkOutcome, Location,
    ResolveError, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("catalog-exercise: usage: catalog-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("catalog-exercise: KDLITH_FX_SUITE is not set; run under the harness");
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

    // The link stage runs once the whole document is read, so a reference
    // to a declaration written later still resolves. Under complete scope
    // its duplicate and unresolved diagnostics fail the reading; the
    // outcomes are reported, not treated as a read failure.
    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data])
        .unwrap_or_else(|error| {
            eprintln!("catalog-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut entries: Vec<&Entry> = Vec::new();
    let mut declarations = 0;
    for item in reading.items() {
        match item {
            TypedItem::Record { value, .. } => entries.extend(ItemVariant::<Entry>::variant(value)),
            TypedItem::Declaration { .. } => declarations += 1,
            _ => {}
        }
    }
    println!("read ok entries={} declarations={declarations}", entries.len());
    println!(
        "linked failed={} duplicates={} diagnostics={}",
        reading.failed(),
        reading.duplicates().len(),
        reading.diagnostics().len()
    );

    for entry in &entries {
        let locale = entry.locale.as_ref().map_or("-", |locale| locale.as_str());
        let group = match &entry.group {
            None => "none".to_owned(),
            // `resolve` takes the reference carrier through `Deref`.
            Some(reference) => match reading.resolve::<Group>(reference) {
                Ok(group) => format!("{}->{}", reference.written(), group.name),
                Err(ResolveError::Link { outcome, .. }) => format!(
                    "{} {} {}",
                    reference.written(),
                    outcome_name(&outcome),
                    outcome.code().unwrap_or("none")
                ),
                Err(other) => format!("{} unresolved {other}", reference.written()),
            },
        };
        println!("entry id={} locale={locale} group={group}", entry.id);
    }
    for duplicate in reading.duplicates() {
        println!(
            "duplicate {} {} at {} first {}",
            duplicate.declaration.identity.domain,
            duplicate.declaration.identity.name.0.join("."),
            line(duplicate.declaration.site),
            line(duplicate.first.site),
        );
    }
    for record in reading.diagnostics() {
        println!("diag {}", record.code);
    }
}

fn outcome_name(outcome: &LinkOutcome) -> &'static str {
    match outcome {
        LinkOutcome::Resolved => "resolved",
        LinkOutcome::Unresolved => "unresolved",
        LinkOutcome::Ambiguous => "ambiguous",
        LinkOutcome::UnmetQualifier(_) => "unmet-qualifier",
        _ => "unknown",
    }
}

fn line(site: Option<Location>) -> String {
    site.map_or("?".to_owned(), |site| {
        format!("{}:{}", site.span.start.line, site.span.start.column)
    })
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "catalog-exercise: {stage} rejected the input ({} diagnostics)",
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
