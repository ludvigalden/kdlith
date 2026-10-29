//! Exercise the generated processor of a schema whose panel cases carry
//! children-group rules: load the committed schema through the generated
//! loader, collect a committed data document holding all three panel
//! kinds as a linked reading through the generated typed reader, and
//! report which members each panel bound — the form's buttons and
//! sliders, the menu's items and links, and the static panel none.
//! Every name the exercise spells comes from the generated crate alone.

use std::path::PathBuf;

use casegrouprule_proc::types::Panel;

use casegrouprule_proc::{CheckScope, DiagnosticRecord, Extensions, ItemVariant, TypedItem, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("casegrouprule-exercise: usage: casegrouprule-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("casegrouprule-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let schema = PathBuf::from(suite).join(schema);
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!(
        "schema ok digest={} documents={}",
        definition.digest(),
        definition.documents.len()
    );

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data])
        .unwrap_or_else(|error| {
            eprintln!("casegrouprule-exercise: data read failed: {error}");
            std::process::exit(1);
        });
    let mut panels: Vec<&Panel> = Vec::new();
    for item in reading.items() {
        if let TypedItem::Record { value, .. } = item {
            panels.extend(ItemVariant::<Panel>::variant(value));
        }
    }
    println!("read ok panels={}", panels.len());
    println!("linked failed={}", reading.failed());
    for panel in &panels {
        let buttons = panel.buttons.as_ref().map_or(0, Vec::len);
        let sliders = panel.sliders.as_ref().map_or(0, Vec::len);
        let items = panel.items.as_ref().map_or(0, Vec::len);
        let links = panel.links.as_ref().map_or(0, Vec::len);
        println!(
            "panel {} buttons={} sliders={} items={} links={}",
            panel.id, buttons, sliders, items, links
        );
    }
    for record in reading.diagnostics() {
        println!("diag {record:?}");
    }
}

fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    for record in records {
        eprintln!("casegrouprule-exercise: {stage} rejected: {}", record.code);
    }
    std::process::exit(1);
}
