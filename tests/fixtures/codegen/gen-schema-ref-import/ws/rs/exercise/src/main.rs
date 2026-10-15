//! Exercise the generated processor of an import-carrying schema whose
//! record type holds a `ref<Group>`: load the committed schema through
//! the generated loader, read a committed data document through the
//! generated typed reader, link the reading's declarations, and resolve
//! every entry's group reference against the linked declarations.

use std::path::PathBuf;

use catalog_proc::types::Entry;
use catalog_proc::{Item, load, reader};
use kdlith_engine::{CheckScope, Extensions, Links, TypedItem, Unresolved};

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

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    // Linking is a separate step over the reading's items: every
    // declaration registers under its identity as it streams past, and
    // the records' references resolve once the whole reading is linked,
    // so a reference to a declaration written later still resolves.
    let mut links: Links<Item> = Links::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        links.observe(&item);
        if let TypedItem::Record {
            value: Item::Entry(entry),
            ..
        } = item
        {
            entries.push(entry);
        }
    };
    match reading.read(&[data], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("catalog-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("catalog-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!(
        "read ok entries={} declarations={}",
        entries.len(),
        links.len()
    );

    for entry in &entries {
        let locale = entry.locale.as_ref().map_or("-", |locale| locale.as_str());
        let group = match &entry.group {
            None => "none".to_owned(),
            Some(reference) => match links.resolve(reference) {
                Ok(group) => format!("{}->{}", reference.written(), group.name),
                Err(Unresolved::Missing) => format!("{} unresolved missing", reference.written()),
                Err(Unresolved::Ambiguous { candidates }) => format!(
                    "{} unresolved ambiguous candidates={candidates}",
                    reference.written()
                ),
                Err(other) => format!("{} unresolved {other}", reference.written()),
            },
        };
        println!("entry id={} locale={locale} group={group}", entry.id);
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
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
