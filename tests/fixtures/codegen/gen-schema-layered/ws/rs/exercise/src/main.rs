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
//! by a struct literal. The generated string newtype keys a map looked
//! up by `&str` and formats as its text, and each item names itself
//! through its identity field, or nothing for a record. Last, the
//! reading collected whole iterates its option declarations through
//! the generated type and looks each settled contribution up by
//! target and scope on the reading surface alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use settings_proc::types::{Choice, OptionName, Patch, Setting};
use settings_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, LAYERED, Layered, ReadError, TypedItem, load,
    matches_line, reader, reader_with_contribution_resolver,
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

    // The regex primitive the generated crate re-exports, evaluated
    // with the schema's own ScopeSpelling matcher: a line the pattern
    // covers in full matches, a trailing fragment does not, and a
    // pattern that does not compile fails closed as no match.
    let scope = r#"(?<name>[a-z][a-z0-9-]*)"#;
    println!("matches_line mode={}", matches_line(scope, "mode"));
    println!("matches_line mode!={}", matches_line(scope, "mode!"));
    println!("matches_line unclosed={}", matches_line("(unclosed", "mode"));
    println!("option domain={}", Choice::DOMAIN);
    println!("patches {}", patch_kind(&Patch::Replace(String::from("replace"))));
    println!("patches {}", patch_kind(&Patch::Append(String::from("append"))));

    // The plain reader is resolver-free: contributions stay records.
    let plain = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(data, |sink| plain.read(&[data], sink));
    println!(
        "plain failed={failed} contributions={} resolutions={}",
        tally.contributions,
        tally.resolutions.len()
    );
    for line in &tally.names {
        println!("{line}");
    }
    // The newtype borrows as `str`, so the map answers a plain `&str`.
    let found = |key: &str| tally.options.get(key).map_or("-".to_owned(), usize::to_string);
    let named = tally.options.keys().any(|name| *name == *"mode");
    println!(
        "option get mode={} ghost={} eq={named}",
        found("mode"),
        found("ghost")
    );
    let mut spelled: Vec<String> = tally.options.keys().map(|name| format!("{name}")).collect();
    spelled.sort();
    println!("option names {}", spelled.join(","));

    let layered = reader_with_contribution_resolver(&definition, Arc::new(Layered))
        .unwrap_or_else(|records| reject("reader", &records));
    let (failed, tally) = read(data, |sink| layered.read(&[data], sink));
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

    let (failed, tally) = read(invalid, |sink| layered.read(&[invalid], sink));
    println!(
        "refused failed={failed} resolutions={} codes={}",
        tally.resolutions.len(),
        tally.codes.join(",")
    );

    // The reading collected whole: its declarations iterate as their
    // own generated type, and the settled contribution for a target
    // the reading holds is looked up by target and scope on the
    // reading surface alone — no forward scan joining contributions
    // to their resolutions.
    let collected = layered.collect(&[data]).unwrap_or_else(|error| {
        eprintln!("layered-exercise: collecting {} broke: {error}", data.display());
        std::process::exit(1);
    });
    let choices: Vec<&str> = collected
        .declarations::<Choice>()
        .map(|choice| choice.name.as_str())
        .collect();
    println!("collected choices={}", choices.join(","));
    for item in collected.items() {
        let TypedItem::Contribution(contribution) = item else {
            continue;
        };
        let target = &contribution.target.written;
        let scope = contribution.scope.as_deref().unwrap_or("-");
        let settled = collected.settled_by::<Setting>(&contribution.target, contribution.scope.as_deref());
        let outcome = match settled {
            Some(Ok(setting)) => format!("settled={}", setting.value),
            Some(Err(mismatch)) => format!("mismatch={mismatch}"),
            None => "-".to_owned(),
        };
        println!("settled-by target={target} scope={scope} {outcome}");
    }
}

#[derive(Default)]
struct Tally {
    contributions: usize,
    resolutions: Vec<String>,
    maps: Vec<String>,
    codes: Vec<String>,
    names: Vec<String>,
    options: HashMap<OptionName, usize>,
}

/// Read one document through `drive`, tallying the stream's items.
fn read(
    path: &Path,
    mut drive: impl FnMut(&mut dyn FnMut(TypedItem<Item>)) -> Result<bool, ReadError>,
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
        TypedItem::Record { value, .. } => {
            if let Item::Metadata(metadata) = value {
                tally.maps.push(format!(
                    "metadata pairs {}",
                    pairs(metadata.pairs.iter().map(|(key, tag)| (key, &tag.text)))
                ));
            }
            tally.names.push(format!("record name={}", value.name().unwrap_or("-")));
        }
        TypedItem::Declaration { value, .. } => {
            if let Item::Choice(choice) = value {
                let at = tally.options.len();
                tally.options.insert(choice.name.clone(), at);
            }
            tally.names.push(format!("declaration name={}", value.name().unwrap_or("-")));
        }
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
