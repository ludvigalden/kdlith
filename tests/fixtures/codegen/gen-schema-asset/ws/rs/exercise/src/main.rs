//! Exercise a generated processor's asset surface through its own
//! re-exports alone: a record's asset fields read as the generated
//! asset newtypes, and the reading that resolved them opens their
//! bytes, alike from an embedded source and from a directory. An asset
//! one reading resolved is refused by another reading; so is a
//! directory asset. Every outcome is printed.

use std::path::{Path, PathBuf};

use shelf_proc::{
    Asset, CheckScope, Extensions, Item, ReadOptions, Reading, SourceTree, TypedItem, load_with,
    reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, entry, lib, other] = args.as_slice() else {
        eprintln!("asset-exercise: usage: asset-exercise <schema> <entry> <lib> <other-lib>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();
    let definition = load_with(
        &[schema.as_path()],
        &extensions,
        CheckScope::Complete,
        ReadOptions::new(),
    )
    .unwrap_or_else(|_| std::process::exit(1));
    let read = |tree: SourceTree| -> Reading<Item> {
        reader(&definition)
            .unwrap_or_else(|_| std::process::exit(1))
            .with_source("lib", tree)
            .collect(&[entry.as_path()])
            .unwrap_or_else(|error| {
                eprintln!("asset-exercise: read failed: {error}");
                std::process::exit(1);
            })
    };
    let embedded = read(SourceTree::embedded(files(lib, lib)));
    let dir = read(SourceTree::dir(lib.clone()));
    let foreign = read(SourceTree::dir(other.clone()));
    for (case, reading) in [("embedded", &embedded), ("dir", &dir)] {
        println!("case {case} failed={}", reading.failed());
        for asset in assets(reading) {
            let digest = asset.digest.as_deref().unwrap_or("-");
            println!("  asset {} source={} digest={digest}", asset.location, asset.source);
            match reading.open(&asset) {
                Ok(bytes) => println!("  open ok {:?}", String::from_utf8_lossy(&bytes)),
                Err(error) => println!("  open refused: {}", error.reason),
            }
        }
    }
    println!("case foreign");
    for asset in assets(&dir) {
        match foreign.open(&asset) {
            Ok(_) => println!("  open accepted {}", asset.location),
            Err(error) => println!("  open refused {}: {}", error.location, error.reason),
        }
    }
}

/// The asset fields of a reading's records, in stream order.
fn assets(reading: &Reading<Item>) -> Vec<Asset> {
    let mut out = Vec::new();
    for item in reading.items() {
        if let TypedItem::Record { value, .. } = item {
            match value {
                Item::Doc(doc) => out.push(doc.spec.0.clone()),
                Item::Tree(tree) => out.push(tree.folder.0.clone()),
                _ => {}
            }
        }
    }
    out
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
        } else if let (Ok(relative), Ok(text)) =
            (path.strip_prefix(root), std::fs::read_to_string(&path))
        {
            out.push((relative.to_string_lossy().replace('\\', "/"), text));
        }
    }
    out
}
