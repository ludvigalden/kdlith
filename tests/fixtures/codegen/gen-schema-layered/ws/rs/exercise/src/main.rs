//! Exercise the generated schema processor's contribution resolution
//! from the consuming side: the schema names the shipped `layered`
//! resolver, the program installs it statically through the generated
//! reader_with_contribution_resolver, and the run reports what each
//! settled target resolved to, the ordered maps the settled settings and
//! the node-local metadata record decode to, and how a refused reading
//! fails.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kdlith_engine::{CheckScope, Extensions, FromValue, TypedItem};
use kdlith_model::Value;
use settings_proc::types::Setting;
use settings_proc::{
    Item, LAYERED, Layered, load, reader, reader_with_contribution_resolver,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, invalid] = args.as_slice() else {
        eprintln!("layered-exercise: usage: layered-exercise <schema> <data> <invalid-data>");
        std::process::exit(2);
    };

    // Without the resolver installed the schema's policy names a
    // resolver nothing registered, so loading fails closed.
    match load(&[schema], &Extensions::standard(), CheckScope::Complete) {
        Ok(_) => {
            eprintln!("layered-exercise: the schema loaded without its resolver installed");
            std::process::exit(1);
        }
        Err(records) => println!("uninstalled rejected {}", codes(&records)),
    }

    let extensions = Extensions::standard().with_resolver(LAYERED);
    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    // The plain reader is resolver-free: contributions stay records.
    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(&plain, data);
    println!(
        "plain failed={failed} contributions={} resolutions={}",
        tally.contributions,
        tally.resolutions.len()
    );

    let layered = reader_with_contribution_resolver(&definition, Arc::new(Layered))
        .unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(&layered, data);
    println!(
        "layered failed={failed} contributions={} resolutions={}",
        tally.contributions,
        tally.resolutions.len()
    );
    for line in &tally.resolutions {
        println!("{line}");
    }
    for line in &tally.maps {
        println!("{line}");
    }

    let (failed, tally) = read(&layered, invalid);
    println!(
        "refused failed={failed} resolutions={} codes={}",
        tally.resolutions.len(),
        tally.codes.join(",")
    );
}

#[derive(Default)]
struct Tally {
    contributions: usize,
    resolutions: Vec<String>,
    maps: Vec<String>,
    codes: Vec<String>,
}

fn read(reading: &kdlith_engine::TypedReader<'_, Item>, path: &Path) -> (bool, Tally) {
    let mut tally = Tally::default();
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Contribution(_) => tally.contributions += 1,
        TypedItem::Resolution(resolved) => {
            tally.resolutions.push(format!(
                "resolved target={} value={} layers={}",
                resolved.target.written,
                entry(&resolved.value, "value"),
                resolved.layers.join(",")
            ));
            tally.maps.push(format!(
                "tags target={} {}",
                resolved.target.written,
                setting_tags(&resolved.value)
            ));
        }
        TypedItem::Record {
            value: Item::Metadata(metadata),
            ..
        } => tally.maps.push(format!(
            "metadata pairs {}",
            pairs(metadata.pairs.iter().map(|(key, tag)| (key, &tag.text)))
        )),
        TypedItem::Diagnostic(record) => tally.codes.push(record.code.clone()),
        _ => {}
    };
    match reading.read(&[path], &mut sink) {
        Ok(failed) => (failed, tally),
        Err(error) => {
            eprintln!("layered-exercise: reading {} broke: {error}", path.display());
            std::process::exit(1);
        }
    }
}

/// The string a resolved map value holds under `key`.
fn entry(value: &Value, key: &str) -> String {
    let Value::Map(entries) = value else {
        return "-".to_owned();
    };
    match entries.iter().find(|entry| entry.key == key).map(|entry| &entry.value) {
        Some(Value::String(text)) => text.clone(),
        _ => "-".to_owned(),
    }
}

/// The settled value read as the generated `Setting`, rendering its
/// optional ordered tag map, or `-` when the setting carries no tags.
fn setting_tags(value: &Value) -> String {
    let setting = Setting::from_value(value).unwrap_or_else(|mismatch| {
        eprintln!("layered-exercise: a settled value is not a Setting: {mismatch}");
        std::process::exit(1);
    });
    match &setting.tags {
        Some(tags) => pairs(tags.iter().map(|(key, tag)| (key, &tag.text))),
        None => "-".to_owned(),
    }
}

fn pairs<'a>(entries: impl Iterator<Item = (&'a String, &'a String)>) -> String {
    entries
        .map(|(key, text)| format!("{key}={text}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn codes(records: &[kdlith_diagnostic::DiagnosticRecord]) -> String {
    records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "layered-exercise: {stage} rejected the input ({})",
        codes(records)
    );
    std::process::exit(1);
}
