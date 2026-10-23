//! Exercise the processor generated from a schema whose unions bind
//! bare scalars: read a committed data document through the generated
//! typed reader, printing the variant each scalar decoded as, and
//! require the rejection of a scalar no variant accepts.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use probes_proc::types::{NumOrTemplate, NumOrText, TextOrTemplate};
use probes_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, invalid] = args.as_slice() else {
        eprintln!("probes-exercise: usage: probes-exercise <schema> <data> <invalid-data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut lines: Vec<String> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| match item {
        TypedItem::Record { value, .. } => lines.push(describe(&value)),
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("probes-exercise: the data reading reported error diagnostics: {lines:?}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("probes-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok records={}", lines.len());
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
            eprintln!("probes-exercise: invalid read failed: {error}");
            std::process::exit(1);
        }
    };
    if rejected && codes.iter().any(|code| code == "KDLITH-BND003") {
        println!("rejected KDLITH-BND003");
    } else {
        eprintln!(
            "probes-exercise: the invalid document was not rejected as declared \
             (had_errors={rejected}, codes={codes:?})"
        );
        std::process::exit(1);
    }
}

fn describe(item: &Item) -> String {
    match item {
        Item::Probe(probe) => match &probe.v {
            NumOrTemplate::Number(number) => format!("probe number={number}"),
            NumOrTemplate::Template(text) => format!("probe template={text}"),
        },
        Item::Label(label) => match &label.v {
            NumOrText::Number(number) => format!("label number={number}"),
            NumOrText::Text(text) => format!("label text={text}"),
        },
        Item::Slot(slot) => match &slot.v {
            TextOrTemplate::Text(text) => format!("slot text={text}"),
            TextOrTemplate::Template(text) => format!("slot template={text}"),
        },
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "probes-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
