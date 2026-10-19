//! Exercise the processor generated from the committed constructs schema,
//! read by suite path: load it through the generated loader, print the
//! declared map type's shape, and read a committed data document through
//! the generated typed reader, printing each plan's enum and optional
//! fields.

use std::path::PathBuf;

use constructs_proc::types::{Labels, Plan, Tier};
use constructs_proc::{CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader};
use kdlith_engine::{LogicalType, Shaped, TypeShape};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("constructs-exercise: usage: constructs-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("constructs-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let schema = PathBuf::from(suite).join(schema);
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    if let TypeShape::Map { name, key, value } = Labels::SHAPE {
        println!("map-type {name} expression={} key={} value={}", Labels::expression(), key(), value());
    }
    let members: Vec<&str> = Tier::MEMBERS.iter().map(|(member, _)| *member).collect();
    println!("enum-type Tier members=[{}]", members.join(","));

    // Building the reader checks every generated shape, the map type's
    // included, against the compiled definition.
    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(plan) = ItemVariant::<Plan>::variant(&value)
        {
            lines.push(describe(plan));
        }
    };
    match reading.read(&[data.as_path()], &mut sink) {
        Ok(errors) => println!("read errors={errors} plans={}", lines.len()),
        Err(error) => {
            eprintln!("constructs-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

fn describe(plan: &Plan) -> String {
    let day = |day: &Option<constructs_proc::types::Day>| {
        day.as_ref().map_or("-".to_owned(), |day| day.as_str().to_owned())
    };
    let text = |text: &Option<String>| text.clone().unwrap_or_else(|| "-".to_owned());
    format!(
        "plan tier={} start={} end={} owner={} contact={}",
        plan.tier.member(),
        day(&plan.start),
        day(&plan.end),
        text(&plan.owner),
        text(&plan.contact),
    )
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "constructs-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
