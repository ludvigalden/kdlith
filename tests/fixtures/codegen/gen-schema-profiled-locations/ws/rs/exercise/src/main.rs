//! Exercise the generated processor of a schema whose profiles declare
//! its locations rule: load the committed schema through the generated
//! loader, then read committed data documents through the generated
//! typed reader under each host input the builder surface takes, and
//! print every item the reading streams with the reading's outcome.

use std::path::{Path, PathBuf};

use kdlith_engine::{CheckScope, Extensions, ReadOptions, SourceTree, TypedItem};
use survey_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, entry, audit, parts] = args.as_slice() else {
        eprintln!("profiled-locations-exercise: usage: profiled-locations-exercise <schema> <entry> <audit> <parts>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let plain = || reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let parts = || SourceTree::dir(parts.clone());

    // The entry document's file name selects the profile whose rule
    // admits its pattern: the named source alone resolves it.
    run("sourced", &plain().with_source("parts", parts()), entry);
    run(
        "sourced-options",
        &plain().with_options(ReadOptions::new().with_source("parts", parts())),
        entry,
    );
    // No rule or default selects a profile for the audit document: only
    // an explicitly named one admits it.
    run("unprofiled", &plain(), audit);
    run("profiled", &plain().with_profile("audit"), audit);
    run(
        "profiled-options",
        &plain().with_options(ReadOptions::new().with_profile("audit")),
        audit,
    );
    // The import fails at its site when the source is not registered, or
    // when the named profile declares no rule admitting the pattern.
    run("unsourced", &plain(), entry);
    run("ruleless-profile", &plain().with_profile("audit").with_source("parts", parts()), entry);
    // An invalid source fails before any document.
    run("invalid-source", &plain().with_source("Parts", parts()), entry);
}

/// Read one entry document and print the stream, then the outcome.
fn run(case: &str, reading: &kdlith_engine::TypedReader<'_, Item>, entry: &Path) {
    println!("case {case}");
    let mut sink = |item: TypedItem<Item>| {
        if let Some(line) = describe(item) {
            println!("  {line}");
        }
    };
    match reading.read(&[entry], &mut sink) {
        Ok(failed) => println!("  failed={failed}"),
        Err(error) => {
            eprintln!("profiled-locations-exercise: {case}: read failed: {error}");
            std::process::exit(1);
        }
    }
}

/// One stream item as a line; scope completions are not printed, and
/// the sources item shows each source's name, kind, and pin state.
fn describe(item: TypedItem<Item>) -> Option<String> {
    Some(match item {
        TypedItem::Document { path, .. } => format!("document {path}"),
        TypedItem::Import(request) => format!(
            "import {} targets=[{}]",
            request.expression,
            request.targets.join(",")
        ),
        TypedItem::Record { value, .. } => match value {
            Item::Thing(thing) => format!("record Thing name={}", thing.name),
            _ => "record other".to_owned(),
        },
        TypedItem::Diagnostic(record) => format!(
            "diagnostic {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        ),
        TypedItem::ScopeComplete(_) => return None,
        TypedItem::Sources(set) => {
            let used: Vec<String> = set
                .sources
                .iter()
                .map(|source| {
                    let name = source.name.as_deref().unwrap_or("(local)");
                    let pinned = source.pinned.map_or("-", |p| if p { "pinned" } else { "unpinned" });
                    format!("{name}:{:?}:{pinned}", source.kind)
                })
                .collect();
            format!("sources [{}]", used.join(","))
        }
        _ => "other".to_owned(),
    })
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "profiled-locations-exercise: {stage} rejected the input ({} diagnostics)",
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
