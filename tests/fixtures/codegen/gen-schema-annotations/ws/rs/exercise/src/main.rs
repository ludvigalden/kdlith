//! Exercise the processor generated from a schema whose node
//! declarations carry annotations: the per-node annotation constants are
//! typed by each annotation's declared type and hold the declared values,
//! and the generated reader binds the annotated nodes as records.

use std::path::PathBuf;

use annotated_proc::bindings::{EVAL_CLASS, EVAL_TIER, WAIT_CLASS};
use annotated_proc::types::{BrowserClass, Step};
use annotated_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("annotated-exercise: usage: annotated-exercise <schema> <data>");
        std::process::exit(2);
    };

    // The constants are typed by the declared annotation types: the
    // generated enum for class, u64 for tier.
    let class: BrowserClass = EVAL_CLASS;
    let tier: u64 = EVAL_TIER;
    println!(
        "constants eval.class={} eval.tier={tier} wait.class={}",
        class.member(),
        WAIT_CLASS.member()
    );

    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut steps: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(step) = ItemVariant::<Step>::variant(&value)
        {
            steps.push(step.name.clone());
        }
    };
    match reading.read(&[data.as_path()], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("annotated-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("annotated-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok steps=[{}]", steps.join(","));
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "annotated-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
