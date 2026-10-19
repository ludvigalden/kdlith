//! Exercise the generated processor of a schema whose context control
//! names the `copy-fields` handler: the consumer supplies the handler's
//! implementation, admits its name at load, and installs it on the
//! generated typed reader, so the stamp record reads the locale the
//! handler copied into context. Loading without admitting the name fails
//! closed.

use std::path::PathBuf;
use std::sync::Arc;

use kdlith_engine::{HandlerCall, HandlerEffect, Handlers};
use stamp_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

const COPY_FIELDS: &str = "copy-fields";

/// Set each context slot the node's fields name, from that field's value.
fn copy_fields(call: &HandlerCall<'_>) -> Vec<HandlerEffect> {
    call.fields
        .iter()
        .map(|(name, value)| HandlerEffect::Set {
            slot: name.clone(),
            value: value.clone(),
        })
        .collect()
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("handler-exercise: usage: handler-exercise <schema> <data>");
        std::process::exit(2);
    };
    match load(&[schema], &Extensions::standard(), CheckScope::Complete) {
        Ok(_) => {
            eprintln!("handler-exercise: the schema loaded without the handler admitted");
            std::process::exit(1);
        }
        Err(records) => {
            let codes: Vec<&str> = records.iter().map(|record| record.code.as_str()).collect();
            println!("load without handler rejected codes={}", codes.join(","));
        }
    }

    let handlers = Handlers::new().with_handler(COPY_FIELDS, Arc::new(copy_fields));
    let extensions = Extensions::standard().with_installed_handlers(&handlers);
    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok");
    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .with_handlers(handlers);
    let mut lines: Vec<String> = Vec::new();
    let mut sink = |item: TypedItem<Item>| match item {
        TypedItem::Record {
            value: Item::Stamp(stamp),
            ..
        } => lines.push(format!("stamp at={}", stamp.at.as_str())),
        TypedItem::Record { .. } | TypedItem::Declaration { .. } => lines.push("other".to_owned()),
        TypedItem::Diagnostic(record) => lines.push(format!("diagnostic {}", record.code)),
        _ => {}
    };
    match reading.read(&[data], &mut sink) {
        Ok(errors) => println!("read errors={errors} lines={}", lines.len()),
        Err(error) => {
            eprintln!("handler-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!("handler-exercise: {stage} rejected the input ({} diagnostics)", records.len());
    for record in records {
        eprintln!("  {} {}", record.code, record.message.as_deref().unwrap_or(""));
    }
    std::process::exit(1);
}
