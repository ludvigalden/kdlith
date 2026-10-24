//! Exercise the generated processor of a schema declaring named
//! reference types: load the committed linking schema through the
//! generated loader, read committed linking data documents through the
//! generated typed reader, and print each reference a record holds with
//! its lookup domain, name segments, qualifiers, spelling, and site; then
//! collect each document as a linked reading and print the link stage's
//! outcome for every reference, its diagnostics, and what `resolve` and
//! `settled` return for each record's reference, with the gate list
//! `link_of` reports for every reference that resolved; reference
//! newtypes pass to `resolve` and call the carrier's methods through
//! `Deref`, with no `.0`; finally read each
//! document again and print every declaration's provenance: its shown
//! path and line, and the namespace of the import that loaded it. Every name
//! the exercise spells comes from the generated crate alone.

use std::path::{Path, PathBuf};

use linking_proc::types::{
    ConfigPathRef, ConfigVarDef, ConfigVarRef, Depends, InputSetDef, InputSetRef, Pin, Takes,
    ToolDef, ToolRef, Use,
};
use linking_proc::{
    CheckScope, DiagnosticRecord, Extensions, Item, ItemVariant, LinkOutcome, Location, Origin,
    Reading, Ref, ResolveError, TypedItem, load, reader,
};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data @ ..] = args.as_slice() else {
        eprintln!("reference-type-exercise: usage: reference-type-exercise <suite-schema> <suite-data>...");
        std::process::exit(2);
    };
    // Schema and data are the committed suite files themselves, never
    // copies: their paths are relative to the suite root the harness
    // exports.
    let Some(suite) = std::env::var_os("KDLITH_FX_SUITE") else {
        eprintln!("reference-type-exercise: KDLITH_FX_SUITE is not set; run under the harness");
        std::process::exit(2);
    };
    let suite = PathBuf::from(suite);
    let extensions = Extensions::standard();

    let definition = load(&[suite.join(schema).as_path()], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());
    for (name, lookup, qualifiers) in [
        ("ConfigVarRef", ConfigVarRef::LOOKUP, ConfigVarRef::QUALIFIERS),
        ("ConfigPathRef", ConfigPathRef::LOOKUP, ConfigPathRef::QUALIFIERS),
        ("ToolRef", ToolRef::LOOKUP, ToolRef::QUALIFIERS),
        ("InputSetRef", InputSetRef::LOOKUP, InputSetRef::QUALIFIERS),
    ] {
        println!("reference-type {name} lookup={lookup} qualifiers={qualifiers:?}");
    }

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    for document in data {
        let mut lines: Vec<String> = Vec::new();
        let mut sink = |item: TypedItem<Item>| {
            let value = match item {
                TypedItem::Record { value, .. } | TypedItem::Declaration { value, .. } => value,
                _ => return,
            };
            if let Some(line) = describe(&value) {
                lines.push(line);
            }
        };
        let path = suite.join(document);
        match reading.read(&[path.as_path()], &mut sink) {
            Ok(errors) => println!("read {} errors={errors} references={}", document.display(), lines.len()),
            Err(error) => fail("data read", &error),
        }
        for line in &lines {
            println!("{line}");
        }
    }

    // The same documents as linked readings: the link stage decides every
    // outcome, and an unresolved reference is an error, never a default.
    let complete = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Complete);
    let local = reader(&definition)
        .unwrap_or_else(|records| reject("reader", &records))
        .link(CheckScope::Local);
    for document in data {
        let path = suite.join(document);
        let linked = complete
            .collect(&[path.as_path()])
            .unwrap_or_else(|error| fail("linked read", &error));
        print_linked(document, &linked);
        let local = local
            .collect(&[path.as_path()])
            .unwrap_or_else(|error| fail("local linked read", &error));
        println!(
            "linked-local {} failed={} diagnostics={}",
            document.display(),
            local.failed(),
            local.diagnostics().len()
        );
    }

    // Each declaration's provenance: the document it was read from as the
    // reading shows it, the line it was written at, and the namespace of the
    // import that loaded its document. Read by suite-relative path from
    // the suite root, so the shown path is the one `vet data` reports
    // there.
    if let Err(error) = std::env::set_current_dir(&suite) {
        fail("entering the suite root", &error);
    }
    for document in data {
        let mut lines: Vec<String> = Vec::new();
        let mut sink = |item: TypedItem<Item>| {
            if let TypedItem::Declaration {
                identity,
                type_name,
                site,
                origin,
                ..
            } = item
            {
                let name = identity.name.0.join(".");
                lines.push(provenance(&type_name, &name, site, &origin));
            }
        };
        if let Err(error) = reading.read(&[document.as_path()], &mut sink) {
            fail("provenance read", &error);
        }
        println!("provenance {} declarations={}", document.display(), lines.len());
        for line in &lines {
            println!("{line}");
        }
    }
}

/// One declaration's provenance line: `path:line` of where it was
/// written, and the import namespace of its document (`-` for none).
fn provenance(type_name: &str, name: &str, site: Option<Location>, origin: &Origin) -> String {
    let line = site.map_or("?".to_owned(), |site| site.span.start.line.to_string());
    format!(
        "declaration {type_name} {name} at={}:{line} namespace={}",
        origin.path,
        origin.namespace.as_deref().unwrap_or("-"),
    )
}

/// The reference a record holds, matched through the type-level
/// projection so the line does not depend on how variants are keyed;
/// a borrowed reference newtype coerces to its carrier.
fn describe(item: &Item) -> Option<String> {
    if let Some(record) = ItemVariant::<Use>::variant(item) {
        return Some(reference("use", "ConfigVarRef", &record.target));
    }
    if let Some(record) = ItemVariant::<Pin>::variant(item) {
        return Some(reference("pin", "ConfigPathRef", &record.target));
    }
    if let Some(record) = ItemVariant::<Depends>::variant(item) {
        return Some(reference("depends", "ToolRef", &record.target));
    }
    if let Some(record) = ItemVariant::<Takes>::variant(item) {
        return Some(reference("takes", "InputSetRef", &record.target));
    }
    None
}

/// One reference line: qualifiers print in the order the spelling
/// carried them, so the line is deterministic.
fn reference<T>(node: &str, ty: &str, target: &Ref<T>) -> String {
    let qualifiers: Vec<String> = target
        .qualifiers()
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let site = target.site();
    format!(
        "{node} {ty} domain={} name={} qualifiers=[{}] written={} site={}:{}",
        target.domain(),
        target.name().join("."),
        qualifiers.join(","),
        target.written(),
        site.span.start.line,
        site.span.start.column,
    )
}

/// A linked reading: its summary, every link per declared reference type
/// in declaration order, the duplicate declarations, the diagnostics, and
/// each record reference resolved through the reading.
fn print_linked(document: &Path, reading: &Reading<Item>) {
    let Some(report) = reading.link_report() else {
        eprintln!("reference-type-exercise: a linked reading has no link report");
        std::process::exit(1);
    };
    let links: usize = report.types.iter().map(|entry| entry.links.len()).sum();
    let pending = reading
        .items()
        .iter()
        .filter(|item| matches!(item, TypedItem::PendingBinding(_)))
        .count();
    println!(
        "linked {} failed={} links={links} duplicates={} diagnostics={} pending={pending}",
        document.display(),
        reading.failed(),
        reading.duplicates().len(),
        reading.diagnostics().len(),
    );
    for entry in &report.types {
        for link in &entry.links {
            let candidates: Vec<String> = link
                .candidates
                .iter()
                .map(|candidate| {
                    let namespace = candidate
                        .namespace
                        .as_ref()
                        .map_or(String::new(), |namespace| format!("(namespace={namespace})"));
                    format!(
                        "{}{namespace}@{}",
                        candidate.identity.name.0.join("."),
                        candidate.site.map_or("?".to_owned(), |site| at(reading, site)),
                    )
                })
                .collect();
            println!(
                "link {} written={} site={} outcome={} candidates=[{}] gated=[{}]{}",
                entry.name,
                link.reference.written,
                at(reading, link.reference.site),
                outcome(&link.outcome),
                candidates.join(","),
                link.gated.join(","),
                reasons(&link.outcome),
            );
        }
    }
    for duplicate in reading.duplicates() {
        println!(
            "duplicate {} {} at {} first {}",
            duplicate.declaration.identity.domain,
            duplicate.declaration.identity.name.0.join("."),
            duplicate.declaration.site.map_or("?".to_owned(), |site| at(reading, site)),
            duplicate.first.site.map_or("?".to_owned(), |site| at(reading, site)),
        );
    }
    for record in reading.diagnostics() {
        println!("diag {} {}", record.code, diagnostic_at(record));
    }
    for item in reading.items() {
        let TypedItem::Record { value, .. } = item else {
            continue;
        };
        // `&*record.target` reborrows the carrier through `Deref`, and
        // `record.target.site()` calls its method through the newtype.
        if let Some(record) = ItemVariant::<Use>::variant(value) {
            let site = record.target.site();
            resolved(reading, "use", &*record.target, site, config_var, tool);
        } else if let Some(record) = ItemVariant::<Pin>::variant(value) {
            let site = record.target.site();
            resolved(reading, "pin", &*record.target, site, config_var, tool);
        } else if let Some(record) = ItemVariant::<Depends>::variant(value) {
            let site = record.target.site();
            resolved(reading, "depends", &*record.target, site, tool, config_var);
        } else if let Some(record) = ItemVariant::<Takes>::variant(value) {
            let site = record.target.site();
            resolved(reading, "takes", &*record.target, site, input_set, tool);
        }
    }
}

/// What `resolve` returns for a record's reference, with the gate list
/// `link_of` reports when it resolved (`resolve` reports one only on
/// failure), then what `settled` returns for it read as its own target
/// type and as another type.
fn resolved<D, O>(
    reading: &Reading<Item>,
    node: &str,
    target: &Ref<D>,
    site: Location,
    name: fn(&D) -> String,
    other: fn(&O) -> String,
) where
    Item: ItemVariant<D> + ItemVariant<O>,
{
    let site = at(reading, site);
    let resolve = reading.resolve(target).map(|value| {
        let gated = reading
            .link_of(target)
            .map_or("?".to_owned(), |link| link.gated.join(","));
        format!("{} gated=[{gated}]", name(value))
    });
    println!("resolve {node} site={site} -> {}", answer(resolve));
    let settled = reading.settled::<D, D>(target).map(name);
    println!("settled {node} site={site} as-target -> {}", answer(settled));
    let settled = reading.settled::<O, D>(target).map(other);
    println!("settled {node} site={site} as-other -> {}", answer(settled));
}

fn config_var(value: &ConfigVarDef) -> String {
    format!("ConfigVarDef {}.{}", value.namespace, value.local_name)
}

fn tool(value: &ToolDef) -> String {
    format!("ToolDef {}/{}@{}", value.program, value.tool, value.contract)
}

fn input_set(value: &InputSetDef) -> String {
    format!("InputSetDef {}", value.name)
}

fn answer(result: Result<String, ResolveError>) -> String {
    match result {
        Ok(value) => value,
        Err(ResolveError::Link { outcome: failed, gated }) => format!(
            "error {} {} gated=[{}]",
            failed.code().unwrap_or("none"),
            outcome(&failed),
            gated.join(",")
        ),
        Err(ResolveError::NotOfType) => "not-of-type".to_owned(),
        Err(ResolveError::NotLinked) => "not-linked".to_owned(),
        Err(ResolveError::NotLinkedHere) => "not-linked-here".to_owned(),
        Err(other) => format!("error {other}"),
    }
}

fn outcome(outcome: &LinkOutcome) -> &'static str {
    match outcome {
        LinkOutcome::Resolved => "resolved",
        LinkOutcome::Unresolved => "unresolved",
        LinkOutcome::Ambiguous => "ambiguous",
        LinkOutcome::UnmetQualifier(_) => "unmet-qualifier",
        _ => "unknown",
    }
}

fn reasons(outcome: &LinkOutcome) -> String {
    let LinkOutcome::UnmetQualifier(reasons) = outcome else {
        return String::new();
    };
    let reasons: Vec<String> = reasons
        .iter()
        .map(|reason| {
            format!(
                "{} spelled={} declared={}",
                reason.qualifier,
                reason.spelled.as_deref().unwrap_or("none"),
                reason.declared.as_deref().unwrap_or("none"),
            )
        })
        .collect();
    format!(" reasons=[{}]", reasons.join(","))
}

/// A site as `file:line:column`, the file named as the reading showed it
/// without its directory, so the line does not depend on the suite root.
fn at(reading: &Reading<Item>, site: Location) -> String {
    let path = reading
        .documents()
        .get(&site.source)
        .map_or("?", |path| file_name(path));
    format!("{path}:{}:{}", site.span.start.line, site.span.start.column)
}

fn diagnostic_at(record: &DiagnosticRecord) -> String {
    format!(
        "{}:{}:{}",
        record.source_path.as_deref().map_or("?", file_name),
        record.line.unwrap_or(0),
        record.col.unwrap_or(0),
    )
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn fail(stage: &str, error: &dyn std::fmt::Display) -> ! {
    eprintln!("reference-type-exercise: {stage} failed: {error}");
    std::process::exit(1);
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    eprintln!(
        "reference-type-exercise: {stage} rejected the input ({} diagnostics)",
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
