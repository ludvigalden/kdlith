//! Embed relmap's changelog export targets and presented version when
//! relmap is found; without it the build still succeeds on the Cargo
//! version. A relmap that is found but fails fails the build.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const COMPONENT: &str = "tools/kdlith";

fn main() {
    println!("cargo::rustc-check-cfg=cfg(kdlith_changelog)");
    println!("cargo::rerun-if-env-changed=RELMAP_BIN");
    println!("cargo::rerun-if-env-changed=PATH");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it"));
    let export = out_dir.join("relmap");
    let state = out_dir.join("relmap-state.json");
    if export.exists() {
        fs::remove_dir_all(&export).unwrap_or_else(|e| panic!("cannot clear {export:?}: {e}"));
    }
    if state.exists() {
        fs::remove_file(&state).unwrap_or_else(|e| panic!("cannot clear {state:?}: {e}"));
    }
    let Some(relmap) = locate() else {
        fallback("relmap was not found (set RELMAP_BIN or put relmap on PATH)");
        return;
    };
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let root = manifest.join("../../..");
    if !root.join("map").is_dir() || !root.join("changelog").is_dir() {
        fallback("the source tree has no map/ and changelog/ for relmap to read");
        return;
    }
    let root = fs::canonicalize(&root).unwrap_or(root);
    watch(&root, &relmap);

    fs::create_dir_all(&export).unwrap_or_else(|e| panic!("cannot create {export:?}: {e}"));

    let presented = relmap_run(&relmap, &root, &["version", COMPONENT]);
    let presented = String::from_utf8(presented).expect("relmap prints UTF-8");
    let presented = presented.trim();
    assert!(
        !presented.is_empty() && !presented.contains(char::is_whitespace),
        "relmap version {COMPONENT} printed no single version: {presented:?}"
    );
    let json = relmap_run(&relmap, &root, &["version", COMPONENT, "--json"]);
    fs::write(&state, json).unwrap_or_else(|e| panic!("cannot write {state:?}: {e}"));
    // A bare export writes every target, matching relmap's own default.
    relmap_run(
        &relmap,
        &root,
        &[
            OsStr::new("export"),
            OsStr::new("--out"),
            export.as_os_str(),
            OsStr::new("--state"),
            state.as_os_str(),
        ],
    );
    for name in [
        "CHANGELOG.md",
        "changelog.ndjson",
        "changelog.index.json",
        "changelog.metadata.json",
    ] {
        let path = export.join(name);
        assert!(path.is_file(), "relmap export wrote no {path:?}");
    }

    println!("cargo::rustc-cfg=kdlith_changelog");
    println!("cargo::rustc-env=KDLITH_PRESENTED_VERSION={presented}");
}

/// `RELMAP_BIN` when set and non-empty, nothing when set empty, else
/// the first executable `relmap` on `PATH`. A `RELMAP_BIN` that names
/// no executable file fails the build.
fn locate() -> Option<PathBuf> {
    if let Some(named) = env::var_os("RELMAP_BIN") {
        if named.is_empty() {
            return None;
        }
        let path = PathBuf::from(named);
        assert!(
            executable(&path),
            "RELMAP_BIN names no executable file: {path:?}"
        );
        return Some(path);
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join("relmap"))
        .find(|candidate| executable(candidate))
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Rebuild when the declarations, relmap itself, or the commit the
/// presented version is judged at change.
fn watch(root: &Path, relmap: &Path) {
    for path in [
        root.join("map"),
        root.join("changelog"),
        relmap.to_path_buf(),
    ] {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    let git = root.join(".git");
    let head = git.join("HEAD");
    let Ok(text) = fs::read_to_string(&head) else {
        return;
    };
    println!("cargo::rerun-if-changed={}", head.display());
    if let Some(name) = text.trim().strip_prefix("ref: ") {
        for path in [git.join(name), git.join("packed-refs")] {
            if path.is_file() {
                println!("cargo::rerun-if-changed={}", path.display());
            }
        }
    }
}

/// Run one relmap verb at the repository root; its stdout on success.
fn relmap_run<S: AsRef<OsStr>>(relmap: &Path, root: &Path, args: &[S]) -> Vec<u8> {
    let output = Command::new(relmap)
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {relmap:?}: {e}"));
    let shown: Vec<_> = args
        .iter()
        .map(|arg| arg.as_ref().to_string_lossy())
        .collect();
    assert!(
        output.status.success(),
        "relmap {} failed ({}):\n{}",
        shown.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn fallback(reason: &str) {
    println!(
        "cargo::warning={reason}: building without changelog data; kdlith presents the Cargo version {}",
        env::var("CARGO_PKG_VERSION").unwrap_or_default()
    );
}
