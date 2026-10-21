//! Exercise the generated processor of a schema binding one type from
//! several differently named nodes: load the committed schema through
//! the generated loader, read a committed data document through the
//! generated typed reader, and print each item under the variant its
//! node reads as, matching the grouped Item enum exhaustively with no
//! wildcard arm, with the type-level projection every variant shares;
//! the enum tone formats as the member its definition spells.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, ItemVariant, TypedItem};
use shop_proc::types::Widget;
use shop_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("node-keyed-exercise: usage: node-keyed-exercise <suite-schema> <data>");
        std::process::exit(2);
    };
    // The schema is the committed suite file itself, never a copy: its
    // path is relative to the suite root the harness exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("node-keyed-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let schema = PathBuf::from(suite).join(schema);
    let extensions = Extensions::standard();

    let definition = load(&[schema.as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        let value = match item {
            TypedItem::Record { value, .. } | TypedItem::Declaration { value, .. } => value,
            _ => return,
        };
        let projected = ItemVariant::<Widget>::variant(&value)
            .map_or_else(|| "-".to_owned(), |widget| widget.id.clone());
        let line = match &value {
            Item::Widget(widget) => format!("widget id={} tone={}", widget.id, widget.tone),
            Item::Gadget(widget) => format!("gadget id={} tone={}", widget.id, widget.tone),
            Item::Part(widget) => format!("part id={} tone={}", widget.id, widget.tone),
            Item::Bin(bin) => format!("bin name={}", bin.name),
        };
        lines.push(format!("{line} widget={projected}"));
    };
    match reading.read(&[data], &mut sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("node-keyed-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("node-keyed-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok items={}", lines.len());
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "node-keyed-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    for record in records {
        eprintln!(
            "  {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        );
    }
    std::process::exit(1);
}
