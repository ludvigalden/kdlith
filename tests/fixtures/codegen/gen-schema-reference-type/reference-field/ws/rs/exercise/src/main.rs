//! Exercise the generated processor of a schema whose record field is
//! typed by a qualified reference type: load the committed schema
//! through the generated loader, read the committed data document
//! through the generated typed reader, and print the reference the
//! record holds with its qualifier.

use std::path::PathBuf;

use config_proc::types::{ConfigVarRef, Use};
use config_proc::{Item, load, reader};
use kdlith_engine::{CheckScope, Extensions, ItemVariant, TypedItem};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("reference-field-exercise: usage: reference-field-exercise <suite-schema> <suite-data>");
        std::process::exit(2);
    };
    // Schema and data are the committed suite files themselves, never
    // copies: their paths are relative to the suite root the harness
    // exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("reference-field-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let suite = PathBuf::from(suite);
    let extensions = Extensions::standard();

    let definition = load(&[suite.join(schema).as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    println!(
        "reference-type ConfigVarRef lookup={} qualifiers={:?}",
        ConfigVarRef::LOOKUP,
        ConfigVarRef::QUALIFIERS
    );

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Record { value, .. } = item
            && let Some(record) = ItemVariant::<Use>::variant(&value)
        {
            let target = &record.target.0;
            let qualifiers: Vec<String> = target
                .qualifiers()
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect();
            lines.push(format!(
                "use domain={} name={} qualifiers=[{}] written={}",
                target.domain(),
                target.name().join("."),
                qualifiers.join(","),
                target.written(),
            ));
        }
    };
    let path = suite.join(data);
    match reading.read_local(&[path.as_path()], &mut sink) {
        Ok(errors) => println!("read errors={errors} references={}", lines.len()),
        Err(error) => {
            eprintln!("reference-field-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "reference-field-exercise: {stage} rejected the input ({} diagnostics)",
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
