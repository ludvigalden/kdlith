//! Exercise the processor generated from a schema declaring one node
//! name under two parents, both declarations carrying the same
//! annotation: the first declaration in contribution order keeps
//! `ITEM_DENSITY`, the second takes the ordinal `ITEM_DENSITY_2`, and the
//! generated reader binds both placements as records.

use std::path::PathBuf;

use panels_proc::bindings::{
    DRAWER_DENSITY, ITEM_DENSITY, ITEM_DENSITY_2, ITEM_WEIGHT, PANEL_DENSITY, PANEL_WEIGHT,
};
use panels_proc::types::{Density, Item};
use panels_proc::{
    CheckScope, DiagnosticRecord, Extensions, ItemKind, ItemVariant, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("panels-exercise: usage: panels-exercise <schema> <data>");
        std::process::exit(2);
    };

    let under_panel: Density = ITEM_DENSITY;
    let under_drawer: Density = ITEM_DENSITY_2;
    let weight: u64 = ITEM_WEIGHT;
    println!(
        "constants item.density={} item.density_2={} item.weight={weight}",
        under_panel.member(),
        under_drawer.member()
    );
    println!(
        "constants panel.density={} panel.weight={PANEL_WEIGHT} drawer.density={}",
        PANEL_DENSITY.member(),
        DRAWER_DENSITY.member()
    );

    let extensions = Extensions::standard();
    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut items: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<ItemKind>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(record) = ItemVariant::<Item>::variant(&value)
        {
            items.push(record.name.clone());
        }
    };
    match reading.read(&[data.as_path()], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("panels-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("panels-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok items=[{}]", items.join(","));
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "panels-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
