//! Exercise the generated processor of a schema declaring a type named
//! `Item`: the generated crate names its processor enum `ItemKind`, so
//! the declared `Item` record type and the enum coexist, and every item
//! reads under the variant its node reads as.

use std::path::PathBuf;

use bind_proc::{CheckScope, DiagnosticRecord, Extensions, ItemKind, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("fallback-exercise: usage: fallback-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    // The schema is the committed binding schema itself, so both paths are
    // read from the suite root.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("fallback-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    if let Err(error) = std::env::set_current_dir(&suite) {
        eprintln!("fallback-exercise: cannot enter the suite root: {error}");
        std::process::exit(1);
    }
    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<ItemKind>| match item {
        TypedItem::Declaration { value, .. } | TypedItem::Record { value, .. } => {
            lines.push(match value {
                ItemKind::Group(group) => {
                    let labels: Vec<String> = group
                        .labels
                        .iter()
                        .map(|(key, label)| format!("{key}:{}", label.text))
                        .collect();
                    format!("group name={} labels=[{}]", group.name, labels.join(","))
                }
                ItemKind::Item(item) => format!(
                    "item id={} kind={} locale={} region={} title={} group={}",
                    item.id,
                    item.kind.member(),
                    item.locale.as_str(),
                    item.region.as_ref().map_or("absent", |region| region.as_str()),
                    item.title.as_deref().unwrap_or("absent"),
                    item.group
                        .as_ref()
                        .map_or("absent".to_owned(), |group| group.name().join(".")),
                ),
                ItemKind::Stamp(stamp) => format!("stamp at={}", stamp.at.as_str()),
                _ => "other".to_owned(),
            });
        }
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("fallback-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("fallback-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
