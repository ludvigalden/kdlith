//! Copies the canonical documents the crate embeds into `OUT_DIR`. They
//! live in the kdlith checkout, so a build without them fails here.

use std::ffi::OsString;
use std::path::PathBuf;

/// The canonical documents' directory, relative to this crate.
const CANONICAL_DIR: &str = "../../kdl/schemas/schema";

/// The documents the crate embeds, in contribution order.
const DOCUMENTS: [&str; 4] = ["types.kdl", "bindings.kdl", "context.kdl", "documents.kdl"];

fn main() {
    let canonical = PathBuf::from(cargo_env("CARGO_MANIFEST_DIR")).join(CANONICAL_DIR);
    let target = PathBuf::from(cargo_env("OUT_DIR")).join("schema");
    if let Err(error) = std::fs::create_dir_all(&target) {
        panic!("cannot create {}: {error}", target.display());
    }
    for name in DOCUMENTS {
        let source = canonical.join(name);
        println!("cargo::rerun-if-changed={}", source.display());
        let text = match std::fs::read(&source) {
            Ok(text) => text,
            Err(error) => panic!(
                "cannot read the canonical document {}: {error}; build this crate from a kdlith checkout",
                source.display()
            ),
        };
        let copy = target.join(name);
        if let Err(error) = std::fs::write(&copy, text) {
            panic!("cannot write {}: {error}", copy.display());
        }
    }
}

/// A variable cargo sets for every build script.
fn cargo_env(name: &str) -> OsString {
    match std::env::var_os(name) {
        Some(value) => value,
        None => panic!("cargo did not set {name}"),
    }
}
