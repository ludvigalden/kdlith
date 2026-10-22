//! Exercise the processor generated from a schema whose recursive
//! container is declared once with `root=#true` beside `parents stack`:
//! the generated reader conforms to the compiled schema, so it admits
//! the container at the document root and nested in itself.

use std::path::PathBuf;

use stacks_proc::types::{Child, Stack};
use stacks_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("stacks-exercise: usage: stacks-exercise <schema> <data>");
        std::process::exit(2);
    };

    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut stacks: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(stack) = ItemVariant::<Stack>::variant(&value)
        {
            stacks.push(titles(stack));
        }
    };
    match reading.read(&[data.as_path()], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("stacks-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("stacks-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok stacks=[{}]", stacks.join(","));
}

/// The titles of a stack's own cards, joined with `+`.
fn titles(stack: &Stack) -> String {
    let items = stack.items.as_deref().unwrap_or_default();
    let cards: Vec<&str> = items
        .iter()
        .filter_map(|child| match child {
            Child::Card(card) => Some(card.title.as_str()),
            Child::Stack(_) => None,
        })
        .collect();
    cards.join("+")
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "stacks-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
