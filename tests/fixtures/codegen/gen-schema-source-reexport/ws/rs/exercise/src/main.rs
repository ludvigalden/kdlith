//! Exercise a generated processor through its own re-exports alone: the
//! crate's manifest names no engine crate. The schema's own $lib/
//! imports load from a directory source, from an embedded one, and from
//! documents held in memory; the typed reader reads a data document
//! importing $lib/ through an embedded source; every outcome is printed.

use std::path::{Path, PathBuf};

use catalog_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ReadOptions, Source, SourceTree, TypedItem,
    load, load_sources, load_sources_with, load_with, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, entry, lib] = args.as_slice() else {
        eprintln!("source-reexport-exercise: usage: source-reexport-exercise <schema> <entry> <lib>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();
    let scope = CheckScope::Complete;
    let embedded = || SourceTree::embedded(files(lib, lib));
    let dir = || SourceTree::dir(lib.clone());
    let text = std::fs::read_to_string(schema).unwrap_or_else(|error| {
        eprintln!("source-reexport-exercise: cannot read {}: {error}", schema.display());
        std::process::exit(2);
    });
    let held = [Source::new("catalog.kdl", text)];

    let paths = [schema.as_path()];
    let lib_dir = || ReadOptions::new().with_source("lib", dir());
    let lib_embedded = || ReadOptions::new().with_source("lib", embedded());
    report("dir", load_with(&paths, &extensions, scope, lib_dir()).map(|d| d.digest()));
    report(
        "embedded",
        load_with(&paths, &extensions, scope, lib_embedded()).map(|d| d.digest()),
    );
    report(
        "held-embedded",
        load_sources_with(&held, &extensions, scope, lib_embedded()).map(|d| d.digest()),
    );
    report("unsourced", load(&paths, &extensions, scope).map(|d| d.digest()));
    report(
        "held-unsourced",
        load_sources(&held, &extensions, scope).map(|d| d.digest()),
    );

    let definition =
        load_with(&paths, &extensions, scope, lib_embedded()).unwrap_or_else(|_| std::process::exit(1));
    let typed = reader(&definition)
        .unwrap_or_else(|_| std::process::exit(1))
        .with_source("lib", embedded());
    println!("case read");
    let mut sink = |item: TypedItem<Item>| {
        if let Some(line) = describe(item) {
            println!("  {line}");
        }
    };
    match typed.read(&[entry.as_path()], &mut sink) {
        Ok(failed) => println!("  failed={failed}"),
        Err(error) => {
            eprintln!("source-reexport-exercise: read failed: {error}");
            std::process::exit(1);
        }
    }
}

/// One load's outcome: the definition's digest, or each diagnostic the
/// load reported.
fn report(case: &str, loaded: Result<String, Vec<DiagnosticRecord>>) {
    println!("case {case}");
    match loaded {
        Ok(digest) => println!("  digest={digest}"),
        Err(records) => {
            for record in records {
                println!("  diagnostic {} {}", record.code, record.message.as_deref().unwrap_or(""));
            }
        }
    }
}

/// Every file under `dir`, keyed by its `/`-separated path under `root`.
fn files(root: &Path, dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map(|read| read.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            out.extend(files(root, &path));
        } else if let (Ok(relative), Ok(text)) = (path.strip_prefix(root), std::fs::read_to_string(&path)) {
            out.push((relative.to_string_lossy().replace('\\', "/"), text));
        }
    }
    out
}

/// One stream item as a line; scope completions are not printed.
fn describe(item: TypedItem<Item>) -> Option<String> {
    Some(match item {
        TypedItem::Document { path, .. } => format!("document {path}"),
        TypedItem::Import(request) => format!(
            "import {} targets=[{}]",
            request.expression,
            request.targets.join(",")
        ),
        TypedItem::Record { value, .. } => match value {
            Item::Product(product) => format!("record Product name={}", product.name),
            _ => "record other".to_owned(),
        },
        TypedItem::Diagnostic(record) => format!(
            "diagnostic {} {}",
            record.code,
            record.message.as_deref().unwrap_or("")
        ),
        TypedItem::ScopeComplete(_) => return None,
        _ => "other".to_owned(),
    })
}
