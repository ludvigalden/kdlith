//! Exercise the processor generated from a schema whose unions hold a
//! reference-typed variant: read a committed data document through the
//! generated typed reader, printing the variant each scalar decoded as
//! and the reference a routed spelling carries — exactly as binding
//! routed it — and require the rejection of spellings no variant
//! accepts.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use union_ref_proc::types::{NumOrTarget, TextOrPinned};
use union_ref_proc::{Item, Ref, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, invalid] = args.as_slice() else {
        eprintln!("union-ref-exercise: usage: union-ref-exercise <schema> <data> <invalid-data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut lines: Vec<String> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| match item {
        TypedItem::Declaration { value, .. } => lines.push(declared(&value)),
        TypedItem::Record { value, .. } => lines.push(describe(&value)),
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("union-ref-exercise: the data reading reported error diagnostics: {lines:?}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("union-ref-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok items={}", lines.len());
    for line in &lines {
        println!("{line}");
    }

    let mut codes: Vec<String> = Vec::new();
    let mut invalid_sink = |item: TypedItem<Item>| {
        if let TypedItem::Diagnostic(record) = item {
            codes.push(record.code.clone());
        }
    };
    let rejected = match reading.read(&[invalid], &mut invalid_sink) {
        Ok(had_errors) => had_errors,
        Err(error) => {
            eprintln!("union-ref-exercise: invalid read failed: {error}");
            std::process::exit(1);
        }
    };
    if rejected && codes.iter().filter(|code| code.as_str() == "KDLITH-BND003").count() == 2 {
        println!("rejected KDLITH-BND003 x2");
    } else {
        eprintln!(
            "union-ref-exercise: the invalid document was not rejected as declared \
             (had_errors={rejected}, codes={codes:?})"
        );
        std::process::exit(1);
    }
}

/// The declaration the document spells, named through `Item::name`.
fn declared(item: &Item) -> String {
    format!("declared name={}", item.name().unwrap_or("?"))
}

/// One bound item: the variant its scalar decoded as, and a routed
/// reference's domain, name, and spelling — the reference binding
/// stored, read back through the generated reader.
fn describe(item: &Item) -> String {
    match item {
        Item::TargetDef(_) => declared(item),
        Item::Count(count) => match &count.v {
            NumOrTarget::Number(number) => format!("count number={number}"),
            NumOrTarget::Target(reference) => reference_line("count", "target", &*reference),
        },
        Item::Note(note) => match &note.v {
            TextOrPinned::Text(text) => format!("note text={text}"),
            TextOrPinned::Pinned(reference) => reference_line("note", "pinned", &*reference),
        },
    }
}

fn reference_line<T>(node: &str, variant: &str, reference: &Ref<T>) -> String {
    format!(
        "{node} {variant} domain={} name={} written={}",
        reference.domain(),
        reference.name().join("."),
        reference.written(),
    )
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "union-ref-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
