//! Exercise the generated processor's shared diagnostic rendering: read
//! a committed data document that carries an unbound property through
//! the generated reader, print each diagnostic record as
//! `DiagnosticRecord::render` renders it — the presentation the
//! re-exported type itself now owns — and print what the CLI's `vet
//! data` writes to stderr for the same document, proving the typed
//! surface and the CLI produce the same bytes. Every kdlith name the
//! exercise spells comes from the generated crate alone; the CLI is
//! reached only as the harness-exported binary.

use std::path::PathBuf;
use std::process::Command;

use render_proc::{CheckScope, DiagnosticRecord, Extensions, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("render-diagnostic-exercise: usage: render-diagnostic-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(&[schema.as_path()], &Extensions::standard(), CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok");

    let reading = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete)
        .collect(&[data.as_path()])
        .unwrap_or_else(|error| {
            eprintln!("render-diagnostic-exercise: reading broke: {error}");
            std::process::exit(1);
        });
    let records: Vec<&DiagnosticRecord> = reading.diagnostics().iter().collect();
    if records.len() != 1 {
        eprintln!(
            "render-diagnostic-exercise: expected one diagnostic, saw {}",
            records.len()
        );
        std::process::exit(1);
    }
    let mut typed = String::new();
    println!("typed:");
    for record in &records {
        let rendered = record.render();
        println!("{rendered}");
        typed.push_str(&rendered);
        typed.push('\n');
    }

    let cli = std::env::var_os("KDLITH_FX_KDLITH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("render-diagnostic-exercise: KDLITH_FX_KDLITH is not set");
            std::process::exit(1);
        });
    let output = Command::new(&cli)
        .args([
            "vet",
            "data",
            "--schema",
            &schema.to_string_lossy(),
            &data.to_string_lossy(),
        ])
        .output()
        .unwrap_or_else(|error| {
            eprintln!("render-diagnostic-exercise: cannot run {}: {error}", cli.display());
            std::process::exit(1);
        });
    if !output.status.success() && output.status.code() != Some(1) {
        eprintln!(
            "render-diagnostic-exercise: vet data exited {:?}",
            output.status.code()
        );
        std::process::exit(1);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    print!("cli:\n{stderr}");

    if typed == stderr {
        println!("renderings match");
    } else {
        eprintln!("render-diagnostic-exercise: the CLI stderr differs from the typed rendering");
        eprintln!("typed: {typed:?}");
        eprintln!("cli:   {stderr:?}");
        std::process::exit(1);
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("render-diagnostic-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
