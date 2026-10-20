//! Exercise the generated schema processor of a contribution-only
//! schema: the definition declares no record or declaration node, so
//! the reading Item enum is empty and contributions are the meaningful
//! items. The typed setting projects through the generated
//! Contribution enum, while the untyped note's payload fails closed at
//! the accessor with the named mismatch, and the reading succeeds
//! either way.

use std::path::PathBuf;

use only_proc::{
    CheckScope, Contribution, DiagnosticRecord, Extensions, Item, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("contribution-exercise: usage: contribution-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(&[schema], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut contributions = 0usize;
    let mut lines = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        if !matches!(item, TypedItem::Contribution(_)) {
            return;
        }
        contributions += 1;
        match item.contribution::<Contribution>() {
            Some(Ok(Contribution::Setting(setting))) => lines.push(format!(
                "typed target={} value={}",
                setting.target, setting.value
            )),
            Some(Ok(_)) => lines.push("typed another contribution variant".to_owned()),
            Some(Err(mismatch)) => lines.push(format!("untyped mismatch {mismatch}")),
            None => lines.push("untyped no contribution value".to_owned()),
        }
    };
    let failed = match reading.read(&[data], &mut sink) {
        Ok(failed) => failed,
        Err(error) => {
            eprintln!("contribution-exercise: reading {} broke: {error}", data.display());
            std::process::exit(1);
        }
    };
    println!("read failed={failed} contributions={contributions}");
    for line in &lines {
        println!("{line}");
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("contribution-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
