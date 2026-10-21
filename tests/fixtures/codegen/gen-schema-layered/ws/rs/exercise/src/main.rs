//! Exercise the generated schema processor's contribution resolution
//! from the consuming side: the schema names the shipped `layered`
//! resolver under `selection=union` with a `merge=union` list field and
//! a scope slot, the program installs the resolver statically through
//! the generated reader_with_contribution_resolver, and the run reports
//! what each settled target resolved to — its settled value read as the
//! typed `Setting` through the processor's typed contribution
//! projection, with its application scope, its merged notes, and the
//! layers that reached it — plus the ordered maps the settled settings
//! and the node-local metadata record decode to, how a refused
//! reading fails, and how the exhaustive marker lets the generated
//! union match with no wildcard arm and the generated object rebuild
//! by a struct literal.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use settings_proc::types::{Choice, Patch, Setting};
use settings_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, LAYERED, Layered, TypedItem, load, reader,
    reader_with_contribution_resolver,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data, invalid] = args.as_slice() else {
        eprintln!("layered-exercise: usage: layered-exercise <schema> <data> <invalid-data>");
        std::process::exit(2);
    };

    // Without the resolver installed the schema's policy names a
    // resolver nothing registered, so loading fails closed.
    match load(&[schema], &Extensions::standard(), CheckScope::Complete) {
        Ok(_) => {
            eprintln!("layered-exercise: the schema loaded without its resolver installed");
            std::process::exit(1);
        }
        Err(records) => println!("uninstalled rejected {}", codes(&records)),
    }

    let extensions = Extensions::standard().with_resolver(LAYERED);
    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    println!("option domain={}", Choice::DOMAIN);
    println!("patches {}", patch_kind(&Patch::Replace(String::from("replace"))));
    println!("patches {}", patch_kind(&Patch::Append(String::from("append"))));

    // The plain reader is resolver-free: contributions stay records.
    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(data, |sink| {
        plain
            .read(&[data], sink)
            .map_err(|error| error.to_string())
    });
    println!(
        "plain failed={failed} contributions={} resolutions={}",
        tally.contributions,
        tally.resolutions.len()
    );

    let layered = reader_with_contribution_resolver(&definition, Arc::new(Layered))
        .unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(data, |sink| {
        layered
            .read(&[data], sink)
            .map_err(|error| error.to_string())
    });
    println!(
        "layered failed={failed} contributions={} resolutions={}",
        tally.contributions,
        tally.resolutions.len()
    );
    for line in &tally.resolutions {
        println!("{line}");
    }
    for line in &tally.maps {
        println!("{line}");
    }

    let (failed, tally) = read(invalid, |sink| {
        layered
            .read(&[invalid], sink)
            .map_err(|error| error.to_string())
    });
    println!(
        "refused failed={failed} resolutions={} codes={}",
        tally.resolutions.len(),
        tally.codes.join(",")
    );
}

#[derive(Default)]
struct Tally {
    contributions: usize,
    resolutions: Vec<String>,
    maps: Vec<String>,
    codes: Vec<String>,
}

/// Read one document through `drive`, tallying the stream's items.
fn read(
    path: &Path,
    mut drive: impl FnMut(&mut dyn FnMut(TypedItem<Item>)) -> Result<bool, String>,
) -> (bool, Tally) {
    let mut tally = Tally::default();
    let mut sink = |item: TypedItem<Item>| match &item {
        TypedItem::Diagnostic(record) => tally.codes.push(record.code.clone()),
        TypedItem::Contribution(_) => tally.contributions += 1,
        TypedItem::Resolution(resolved) => {
            let setting = settled(&item);
            // The exhaustive marker lets the generated object be rebuilt
            // by a full struct literal, field for field.
            if rebuilt(&setting) != setting {
                eprintln!("layered-exercise: the rebuilt setting differs");
                std::process::exit(1);
            }
            let target = &resolved.target.written;
            tally.resolutions.push(format!(
                "resolved target={target} value={} scope={} layers={}",
                setting.value,
                scope_of(&setting),
                resolved.layers.join(","),
            ));
            tally.maps.push(format!("tags target={target} {}", tag_texts(&setting)));
            tally.maps.push(format!(
                "notes target={target} {}",
                note_texts(&setting)
            ));
        }
        TypedItem::Record {
            value: Item::Metadata(metadata),
            ..
        } => tally.maps.push(format!(
            "metadata pairs {}",
            pairs(metadata.pairs.iter().map(|(key, tag)| (key, &tag.text)))
        )),
        _ => {}
    };
    match drive(&mut sink) {
        Ok(failed) => (failed, tally),
        Err(error) => {
            eprintln!("layered-exercise: reading {} broke: {error}", path.display());
            std::process::exit(1);
        }
    }
}

/// A settled value read as the generated `Setting` through the
/// processor's typed contribution projection.
fn settled(item: &TypedItem<Item>) -> Setting {
    match item.contribution::<Setting>() {
        Some(Ok(setting)) => setting,
        Some(Err(mismatch)) => {
            eprintln!("layered-exercise: a settled value is not a Setting: {mismatch}");
            std::process::exit(1);
        }
        None => {
            eprintln!("layered-exercise: a resolution item carries no contribution value");
            std::process::exit(1);
        }
    }
}

/// The generated union's variant, matched with no wildcard arm.
fn patch_kind(patch: &Patch) -> &'static str {
    match patch {
        Patch::Replace(_) => "replace",
        Patch::Append(_) => "append",
    }
}

/// The setting rebuilt by a struct literal over every generated field,
/// which compiles only where the object is exhaustive.
fn rebuilt(setting: &Setting) -> Setting {
    Setting {
        scope: setting.scope.clone(),
        target: setting.target.clone(),
        value: setting.value.clone(),
        follows: setting.follows.clone(),
        layer: setting.layer.clone(),
        tags: setting.tags.clone(),
        notes: setting.notes.clone(),
    }
}

/// The settled setting's application scope as written, or `-` when it
/// contributed under none.
fn scope_of(setting: &Setting) -> String {
    match &setting.scope {
        Some(term) => term.0.written().to_owned(),
        None => "-".to_owned(),
    }
}

/// The settled tags rendered as `key=text` pairs, or `-` when the
/// setting carries none.
fn tag_texts(setting: &Setting) -> String {
    match &setting.tags {
        Some(tags) => pairs(tags.iter().map(|(key, tag)| (key, &tag.text))),
        None => "-".to_owned(),
    }
}

/// The settled `merge=union` notes — the lower layers' items with the
/// refinements' appended — as their texts, or `-` when none.
fn note_texts(setting: &Setting) -> String {
    match &setting.notes {
        Some(notes) => notes
            .iter()
            .map(|note| note.text.as_str())
            .collect::<Vec<_>>()
            .join(","),
        None => "-".to_owned(),
    }
}

fn pairs<'a>(entries: impl Iterator<Item = (&'a String, &'a String)>) -> String {
    entries
        .map(|(key, text)| format!("{key}={text}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn codes(records: &[DiagnosticRecord]) -> String {
    records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "layered-exercise: {stage} rejected the input ({})",
        codes(records)
    );
    std::process::exit(1);
}
