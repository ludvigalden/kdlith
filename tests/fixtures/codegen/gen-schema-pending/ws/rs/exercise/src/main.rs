//! Exercise the generated schema processor's typed reading of a dependent
//! value binding whose codec no processor installs: the reading succeeds,
//! the contributions arrive as before, and each deferred payload arrives
//! as a typed pending-binding item right after the contribution holding
//! its reference, never as a validated value.

use std::path::PathBuf;

use deps_proc::{Item, load, reader};
use kdlith_engine::{CheckScope, Extensions, TypedItem};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("pending-exercise: usage: pending-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut order = Vec::new();
    let mut pending = Vec::new();
    let mut sink = |item: TypedItem<Item>| match &item {
        TypedItem::Declaration { .. } => order.push("declaration"),
        TypedItem::Contribution(_) => order.push("contribution"),
        TypedItem::PendingBinding(_) => {
            order.push("pending");
            pending.push(serde_json::to_string(&item).unwrap_or_default());
        }
        _ => {}
    };
    let failed = match reading.read(&[data], &mut sink) {
        Ok(failed) => failed,
        Err(error) => {
            eprintln!("pending-exercise: reading {} broke: {error}", data.display());
            std::process::exit(1);
        }
    };
    println!("read failed={failed} pending={}", pending.len());
    println!("order {}", order.join(","));
    for line in &pending {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("pending-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
