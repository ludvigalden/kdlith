//! Exercise the generated processor of a schema holding each rendered
//! type expression: load the committed schema through the generated
//! loader, read a committed data document through the generated typed
//! reader and print every decoded value, then collect documents as
//! readings linked under complete and local scope and print how each
//! reference-typed map key linked and what it resolves to.

use std::path::{Path, PathBuf};

use expressions_proc::types::{Attr, Keyed, Notes, Rec, RecPick, Tally, TallyV};
use expressions_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, LinkOutcome, Location, Reading,
    Ref, ResolveError, Resolved, TypedItem, load, reader,
};
use kdlith_engine::{LogicalType, Nullable, Shaped, TypeShape};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, unresolved] = args.as_slice() else {
        eprintln!("type-expressions-exercise: usage: type-expressions-exercise <schema> <data> <unresolved-data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    if let TypeShape::Map { name, key, value } = Notes::SHAPE {
        println!("map-type {name} key={} value={}", key(), value());
    }
    println!(
        "inline-unions Rec.pick={} Tally.v={}",
        RecPick::expression(),
        TallyV::expression()
    );

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        let value = match item {
            TypedItem::Record { value, .. } | TypedItem::Declaration { value, .. } => value,
            _ => return,
        };
        lines.extend(describe(&value));
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => fail("data read", &error),
    }
    for line in &lines {
        println!("{line}");
    }

    let complete = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete);
    let local = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Local);
    for document in [data, unresolved] {
        let linked = complete
            .collect(&[document.as_path()])
            .unwrap_or_else(|error| fail("linked read", &error));
        print_linked(document, &linked);
        let local = local
            .collect(&[document.as_path()])
            .unwrap_or_else(|error| fail("local linked read", &error));
        println!(
            "linked-local {} failed={} diagnostics={}",
            file_name(&document.to_string_lossy()),
            local.failed(),
            local.diagnostics().len()
        );
    }
}

/// The lines of one decoded record or declaration, every value as its
/// generated Rust type holds it. Each map entry prints on its own line,
/// so no line depends on the order the document wrote the entries in.
fn describe(item: &Item) -> Vec<String> {
    if let Some(attr) = ItemVariant::<Attr>::variant(item) {
        return vec![format!("attr name={}", attr.name)];
    }
    if let Some(rec) = ItemVariant::<Rec>::variant(item) {
        let mut lines = vec![record(rec)];
        for (day, note) in rec.notes.iter().flat_map(|notes| notes.0.iter()) {
            lines.push(format!("rec name={} notes-entry {}={}", rec.name, day.as_str(), note.text));
        }
        return lines;
    }
    if let Some(keyed) = ItemVariant::<Keyed>::variant(item) {
        let mut lines = vec![format!(
            "keyed name={} values={} plain={}",
            keyed.name,
            keyed.values.len(),
            keyed.plain.len()
        )];
        for (key, value) in &keyed.values {
            let site = key.0.site();
            lines.push(format!(
                "keyed name={} values-entry key={} domain={} name={} site={}:{} value={}",
                keyed.name,
                key.0.written(),
                key.0.domain(),
                key.0.name().join("."),
                site.span.start.line,
                site.span.start.column,
                value.value
            ));
        }
        for (key, value) in &keyed.plain {
            lines.push(format!("keyed name={} plain-entry {key}={}", keyed.name, value.value));
        }
        return lines;
    }
    if let Some(tally) = ItemVariant::<Tally>::variant(item) {
        let v = match &tally.v {
            TallyV::Bool(flag) => format!("bool:{flag}"),
            TallyV::String(text) => format!("string:{text}"),
            _ => "other".to_owned(),
        };
        return vec![format!("tally n={} v={v}", tally.n)];
    }
    Vec::new()
}

/// A record line: an absent optional nullable prints `absent`, an
/// explicit `#null` prints `null`; the serialized record omits an absent
/// field and writes a null one, and a decimal keeps its spelling.
fn record(rec: &Rec) -> String {
    let ratio = match &rec.ratio {
        Nullable::Null => "null".to_owned(),
        Nullable::Value(value) => format!("value:{}", value.as_str()),
    };
    let note = match &rec.note {
        None => "absent".to_owned(),
        Some(Nullable::Null) => "null".to_owned(),
        Some(Nullable::Value(text)) => format!("value:{text}"),
    };
    let pick = match &rec.pick {
        RecPick::U64(number) => format!("u64:{number}"),
        RecPick::String(text) => format!("string:{text}"),
        _ => "other".to_owned(),
    };
    let notes = rec.notes.as_ref().map_or(0, |notes| notes.0.len());
    let json = serde_json::to_value(rec).unwrap_or_else(|error| fail("serializing a record", &error));
    let serialized_note = json.get("note").map_or("omitted".to_owned(), ToString::to_string);
    let serialized_price = json.get("price").map_or("omitted".to_owned(), ToString::to_string);
    format!(
        "rec name={} count={} price={} ratio={ratio} note={note} pick={pick} notes={notes} \
         serialized-note={serialized_note} serialized-price={serialized_price}",
        rec.name,
        rec.count,
        rec.price.as_str(),
    )
}

/// A linked reading: its summary, each link, its diagnostics, and what
/// each reference-typed map key resolves to.
fn print_linked(document: &Path, reading: &Reading<Item>) {
    let Some(report) = reading.link_report() else {
        eprintln!("type-expressions-exercise: a linked reading has no link report");
        std::process::exit(1);
    };
    let links: usize = report.types.iter().map(|entry| entry.links.len()).sum();
    println!(
        "linked {} failed={} links={links} diagnostics={}",
        file_name(&document.to_string_lossy()),
        reading.failed(),
        reading.diagnostics().len(),
    );
    for entry in &report.types {
        for link in &entry.links {
            println!(
                "link {} written={} field={} key={} site={} outcome={} candidates={}",
                entry.name,
                link.reference.written,
                link.field.as_deref().unwrap_or("-"),
                link.key,
                at(reading, link.reference.site),
                outcome(&link.outcome),
                link.candidates.len(),
            );
        }
    }
    for record in reading.diagnostics() {
        println!("diag {} {}", record.code, diagnostic_at(record));
    }
    for item in reading.items() {
        let TypedItem::Record { value, .. } = item else {
            continue;
        };
        let Some(keyed) = ItemVariant::<Keyed>::variant(value) else {
            continue;
        };
        for key in keyed.values.keys() {
            resolved(reading, &key.0);
        }
    }
}

fn resolved(reading: &Reading<Item>, key: &Ref<Attr>) {
    let answer = match reading.resolve(key) {
        Ok(Resolved::Declaration(attr)) => format!("Attr {}", attr.name),
        Ok(Resolved::External) => "external".to_owned(),
        Ok(_) => unreachable!("Resolved holds only a declaration or external"),
        Err(ResolveError::Link { outcome: failed, .. }) => {
            format!("error {} {}", failed.code().unwrap_or("none"), outcome(&failed))
        }
        Err(other) => format!("error {other}"),
    };
    println!("resolve key {} site={} -> {answer}", key.written(), at(reading, key.site()));
}

fn outcome(outcome: &LinkOutcome) -> &'static str {
    match outcome {
        LinkOutcome::Resolved => "resolved",
        LinkOutcome::Unresolved => "unresolved",
        LinkOutcome::Ambiguous => "ambiguous",
        LinkOutcome::UnmetQualifier(_) => "unmet-qualifier",
        _ => "unknown",
    }
}

/// A site as `file:line:column`, the file without its directory.
fn at(reading: &Reading<Item>, site: Location) -> String {
    let path = reading
        .documents()
        .get(&site.source)
        .map_or("?", |path| file_name(path));
    format!("{path}:{}:{}", site.span.start.line, site.span.start.column)
}

fn diagnostic_at(record: &DiagnosticRecord) -> String {
    format!(
        "{}:{}:{}",
        record.source_path.as_deref().map_or("?", file_name),
        record.line.unwrap_or(0),
        record.col.unwrap_or(0),
    )
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn fail(stage: &str, error: &dyn std::fmt::Display) -> ! {
    eprintln!("type-expressions-exercise: {stage} failed: {error}");
    std::process::exit(1);
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "type-expressions-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
