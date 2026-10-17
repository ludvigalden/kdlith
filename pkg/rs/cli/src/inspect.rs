//! `kdlith inspect dialect|schema|data`: read documents through the
//! reference processor and print each item as one JSON line (the source
//! view), then check a definition as `vet` does; diagnostics go to
//! stderr.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus, Severity};
use kdlith_engine::{
    AccumulateOp, CheckScope, ContextPolicy, DeferredBinding, Definition, EffectStep, EntryContext,
    Extensions, ImportDecl, LinkOutcome, Names, Role, ScopeId, ScopeRules, SetFrom, SlotDecl,
    SourceRevisions, Sym, TypeExpr, TypedItem, list_nodes, list_profiles, list_types,
};
use kdlith_model::Location;
use kdlith_superproc::{Item, load_dialect, load_schema_anchored, reader};

use crate::InspectCommand;
use crate::ViewArg;
use crate::report::report_all;
use crate::vet::{Linked, VetTarget, definition_extensions, governing_dialect};

/// Run a parsed `inspect` subcommand. The view matrix is decided before
/// any document is read.
pub(crate) fn run(command: InspectCommand) -> ExitStatus {
    match command {
        InspectCommand::Dialect(args) => match dispatch(args.view, VetTarget::Dialect) {
            Ok(rendering) => inspect_view(
                VetTarget::Dialect,
                args.view,
                rendering,
                args.dialect.as_deref(),
                &paths(&args.files),
                &[],
            ),
            Err(status) => status,
        },
        InspectCommand::Schema(args) => match dispatch(args.view, VetTarget::Schema) {
            Ok(rendering) => inspect_view(
                VetTarget::Schema,
                args.view,
                rendering,
                args.dialect.as_deref(),
                &paths(&args.files),
                &args.anchors,
            ),
            Err(status) => status,
        },
        InspectCommand::Data(args) => {
            let rendering = match dispatch(args.view, VetTarget::Data) {
                Ok(rendering) => rendering,
                Err(status) => return status,
            };
            let mut revisions = SourceRevisions::new();
            match args.profiles.data_options(args.anchors, args.handlers) {
                Ok(options) => crate::vet::stream_data_then(
                    args.schema.as_deref(),
                    &paths(&args.files),
                    &options
                        .with_dialect(args.dialect)
                        .with_scope(crate::vet::scope_of(args.scope)),
                    rendering == Rendering::References,
                    |out, item| {
                        revisions.observe(item);
                        render_data(args.view, &revisions, out, item)
                    },
                    |definition, contexts, linked, status| match (rendering, definition) {
                        (Rendering::Stream, _) => status,
                        (Rendering::References, _) => match linked {
                            Some(linked) => match write_lines(&reference_lines(&linked)) {
                                ExitStatus::Success => status,
                                failed => failed,
                            },
                            None => status,
                        },
                        (Rendering::Entries, Some(definition)) => {
                            match write_lines(&context_lines(definition, contexts)) {
                                ExitStatus::Success => status,
                                failed => failed,
                            }
                        }
                        (Rendering::Entries, None) => status,
                        (Rendering::Definition, Some(definition)) => {
                            match render_definition(args.view, definition) {
                                ExitStatus::Success => status,
                                failed => failed,
                            }
                        }
                        (Rendering::Definition, None) => uncompiled(args.view),
                    },
                ),
                Err(status) => status,
            }
        }
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

/// How a view renders: by reframing the item stream as it is read,
/// from the compiled definition once checking is done, from the
/// contexts the entry documents left once the data is read, or from the
/// reading's link report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rendering {
    Stream,
    Definition,
    Entries,
    References,
}

impl Rendering {
    fn of(view: ViewArg, target: VetTarget) -> Self {
        match (view, target) {
            (ViewArg::Effective, VetTarget::Dialect | VetTarget::Schema) | (ViewArg::Plan, _) => {
                Rendering::Definition
            }
            (ViewArg::Context, _) => Rendering::Entries,
            (ViewArg::References, _) => Rendering::References,
            _ => Rendering::Stream,
        }
    }
}

/// The per-target view matrix, applied before any document is read.
/// bound, context, origins and references describe data binding and
/// linking, so on a definition target they are an argument fault.
fn dispatch(view: ViewArg, target: VetTarget) -> Result<Rendering, ExitStatus> {
    let definition_target = matches!(target, VetTarget::Dialect | VetTarget::Schema);
    match view {
        ViewArg::Bound | ViewArg::Context | ViewArg::Origins | ViewArg::References
            if definition_target =>
        {
            Err(crate::usage::report_inapplicable_view(
                view.name(),
                target_name(target),
            ))
        }
        _ => Ok(Rendering::of(view, target)),
    }
}

/// Write one data item under a stream view, in stream order. bound and
/// effective filter the items; origins writes provenance envelopes from
/// the revisions announced so far, as export does.
fn render_data(
    view: ViewArg,
    revisions: &SourceRevisions,
    out: &mut impl std::io::Write,
    item: &kdlith_model::Item,
) -> std::io::Result<()> {
    use kdlith_model::Item as Data;
    match (view, item) {
        // A pending binding is neither a declaration nor a resolution and
        // has no provenance of its own, so only bound shows it.
        (
            ViewArg::Bound,
            Data::Record(_)
            | Data::Declaration(_)
            | Data::Contribution(_)
            | Data::PendingBinding(_),
        )
        | (ViewArg::Effective, Data::Declaration(_) | Data::Resolution(_))
        | (ViewArg::Source, _) => crate::vet::write_item(out, item),
        (ViewArg::Origins, _) => match revisions.provenance(item) {
            Some(provenance) => {
                let line = serde_json::json!({"item": "origins", "data": provenance});
                serde_json::to_writer(&mut *out, &line)?;
                out.write_all(b"\n")
            }
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

fn target_name(target: VetTarget) -> &'static str {
    match target {
        VetTarget::Dialect => "dialect",
        VetTarget::Schema => "schema",
        VetTarget::Data => "data",
    }
}

fn not_implemented(view: ViewArg) -> ExitStatus {
    eprintln!("kdlith: the {} view is not implemented yet", view.name());
    ExitStatus::Failure
}

/// Read `entries` and their imports as definition documents written in
/// the governing dialect — the bundled one, or the single document
/// `dialect` names — expanding the documents' import expressions
/// against `anchors`, then check them as the `target` definition with
/// the loader `vet` uses. Every item is shown before any compile fault
/// is reported: reader diagnostics, then compile diagnostics, decide the
/// exit together.
pub fn inspect(
    target: VetTarget,
    dialect: Option<&Path>,
    entries: &[&Path],
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    inspect_view(
        target,
        ViewArg::Source,
        Rendering::Stream,
        dialect,
        entries,
        anchors,
    )
}

/// [`inspect`] under `view`. A stream view shows every item, then fails
/// on any error; a definition view prints nothing unless the definition
/// compiled.
fn inspect_view(
    target: VetTarget,
    view: ViewArg,
    rendering: Rendering,
    dialect: Option<&Path>,
    entries: &[&Path],
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    if target == VetTarget::Data {
        eprintln!("kdlith: inspect data binds data documents through vet data");
        return ExitStatus::Usage;
    }
    let extensions = definition_extensions();
    let compiled = match governing_dialect(dialect, &extensions) {
        Ok(compiled) => compiled,
        Err(records) => return report_all(&records),
    };
    let reader = match reader(&compiled) {
        Ok(reader) => reader.with_anchors(anchors),
        Err(records) => return report_all(&records),
    };
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    let mut written = true;
    let mut sink = |item: TypedItem<Item>| {
        if let TypedItem::Diagnostic(record) = &item {
            records.push((**record).clone());
        }
        if rendering == Rendering::Stream {
            written &= write_item(&mut out, &item).is_ok();
        }
    };
    let read = reader.read(entries, &mut sink);
    drop(out);
    match read {
        Ok(_) if written => {
            // The loader re-reads the same documents and stops on the
            // reader's own errors, so compiling after one would only
            // repeat them.
            let mut definition = None;
            if !records.iter().any(|r| r.severity == Severity::Error) {
                match check(target, &compiled, entries, &extensions, anchors) {
                    Ok(checked) => definition = Some(checked),
                    Err(compile) => records.extend(compile),
                }
            }
            let status = report_all(&records);
            match (rendering, definition) {
                // The matrix admits the entries and references renderings
                // on data only.
                (Rendering::Stream | Rendering::Entries | Rendering::References, _) => status,
                (Rendering::Definition, Some(definition)) => render_definition(view, &definition),
                (Rendering::Definition, None) => uncompiled(view),
            }
        }
        Ok(_) => ExitStatus::Failure,
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            ExitStatus::Failure
        }
    }
}

fn uncompiled(view: ViewArg) -> ExitStatus {
    eprintln!(
        "kdlith: the {} view needs the compiled definition, which did not compile",
        view.name()
    );
    ExitStatus::Failure
}

/// Render a definition-rendered view from the compiled definition.
fn render_definition(view: ViewArg, definition: &Definition) -> ExitStatus {
    match view {
        ViewArg::Effective => write_lines(&effective_lines(definition)),
        ViewArg::Plan => write_lines(&plan_lines(definition)),
        _ => not_implemented(view),
    }
}

/// The plan view: one plan-context line with the slots in slot-id order
/// and the namespaces in namespace-id order, then one plan-node line per
/// node in plan order, which is declaration order.
fn plan_lines(def: &Definition) -> Vec<serde_json::Value> {
    use serde_json::json;
    let n = &def.names;
    let context = &def.plan.context;
    let mut slots: Vec<(usize, &SlotDecl)> = def
        .slots
        .iter()
        .filter_map(|slot| Some((context.slot(slot.name)?.index(), slot)))
        .collect();
    slots.sort_by_key(|(id, _)| *id);
    let mut namespaces: Vec<(usize, Sym)> = def
        .slots
        .iter()
        .filter_map(|slot| slot.namespace)
        .filter_map(|ns| Some((context.namespace(ns)?.index(), ns)))
        .collect();
    namespaces.sort_by_key(|(id, _)| *id);
    namespaces.dedup();
    let slot_names = id_names(n, slots.iter().map(|(id, slot)| (*id, slot.name)));
    let namespace_names = id_names(n, namespaces.iter().copied());
    let accumulating: Vec<bool> = slots.iter().map(|(_, s)| s.accumulate.is_some()).collect();
    let names = PlanNames {
        n,
        slots: &slot_names,
        namespaces: &namespace_names,
    };
    let mut lines = vec![json!({"item": "plan-context", "data": {
        "slots": slots.iter().map(|(_, slot)| json!({
            "name": n.resolve(slot.name),
            "type": type_text(n, &slot.ty),
            "namespace": slot.namespace.map(|s| n.resolve(s)),
            "accumulate": slot.accumulate.map(AccumulateOp::name),
        })).collect::<Vec<_>>(),
        "namespaces": namespace_names,
    }})];
    for ((_, decl), node) in def.bindings.iter().zip(def.plan.nodes()) {
        let scope = node.scope.and_then(|id| def.bindings.scope(id));
        let steps = || node.children_effects.iter().chain(&node.following_effects);
        let flags = [
            (
                "record-buffering",
                node.deferred.contains(&DeferredBinding::Contribution)
                    || steps().any(|step| {
                        matches!(step, EffectStep::Set { slot, .. }
                        if accumulating.get(slot.index()).copied().unwrap_or(false))
                    }),
            ),
            (
                "scope-completion",
                scope.is_some_and(|scope| completes(&scope.rules)),
            ),
            ("declaration-lookup", node.declaration),
            ("imported-schema-metadata", !def.imports.is_empty()),
            (
                "deferred-binding",
                node.deferred
                    .iter()
                    .any(|d| matches!(d, DeferredBinding::Reference { .. })),
            ),
            ("global-validation", def.document_rule.is_some()),
            (
                "registered-domain-handlers",
                steps().any(|step| matches!(step, EffectStep::Handler { .. })),
            ),
        ];
        let requires: Vec<&str> = flags
            .iter()
            .filter(|(_, on)| *on)
            .map(|(f, _)| *f)
            .collect();
        lines.push(json!({"item": "plan-node", "data": {
            "name": n.resolve(decl.name),
            "role": node.role.map(Role::name),
            "scope": node.scope.map(ScopeId::index),
            "children-effects": names.effects(&node.children_effects),
            "following-effects": names.effects(&node.following_effects),
            "context-reads": node.context_reads.iter().map(|read| json!({
                "field": n.resolve(read.field),
                "slot": names.slot(read.slot.index()),
                "policy": read.policy.map(policy_name),
            })).collect::<Vec<_>>(),
            "deferred": node.deferred.iter().map(|d| names.deferred(d)).collect::<Vec<_>>(),
            "dependent": node.dependent.as_ref().map(|dep| json!({
                "target": n.resolve(dep.target),
                "member": n.resolve(dep.member),
                "codec": dep.codec.name(),
            })),
            "declaration": node.declaration,
            "requires": requires,
        }}));
    }
    lines
}

/// The context view: one context line per entry document in entry
/// order, every slot in slot-id order, an unset slot with a null value.
fn context_lines(def: &Definition, contexts: &[EntryContext]) -> Vec<serde_json::Value> {
    use serde_json::json;
    let ids = def
        .slots
        .iter()
        .filter_map(|slot| Some((def.plan.context.slot(slot.name)?.index(), slot.name)));
    let names = id_names(&def.names, ids);
    contexts
        .iter()
        .map(|context| {
            let slots: Vec<_> = context
                .values
                .iter()
                .enumerate()
                .map(|(id, held)| {
                    json!({
                        "name": names.get(id).map_or("", String::as_str),
                        "value": held.as_ref().map(|held| json!(held.value)),
                        "layer": held.as_ref().and_then(|held| held.layer.as_deref()),
                    })
                })
                .collect();
            json!({"item": "context", "data": {"entry": context.entry, "slots": slots}})
        })
        .collect()
}

/// The references view: one line per declared reference type in
/// declaration order, each with its links in stream order. A type no
/// reference names still renders, with no links.
fn reference_lines(linked: &Linked<'_>) -> Vec<serde_json::Value> {
    use serde_json::json;
    let site = |site: &Location| {
        json!({
            "source": linked.documents.get(&site.source),
            "span": site.span,
        })
    };
    linked
        .report
        .types
        .iter()
        .map(|declared| {
            let links: Vec<_> = declared
                .links
                .iter()
                .map(|link| {
                    let reference = &link.reference;
                    let mut line = json!({
                        "written": reference.written,
                        "domain": reference.domain,
                        "name": reference.name.0,
                        "qualifiers": reference.qualifiers.iter().map(|(name, value)| {
                            json!({"name": name, "value": value})
                        }).collect::<Vec<_>>(),
                        "context": reference.context,
                        "field": link.field,
                        "site": site(&reference.site),
                        "candidates": link.candidates.iter().map(|candidate| json!({
                            "identity": {
                                "domain": candidate.identity.domain,
                                "name": candidate.identity.name.0,
                            },
                            "alias": candidate.alias,
                            "labels": candidate.labels,
                            "site": candidate.site.as_ref().map(site),
                        })).collect::<Vec<_>>(),
                        "gated": link.gated,
                        "outcome": outcome_name(&link.outcome),
                    });
                    if let LinkOutcome::UnmetQualifier(reasons) = &link.outcome {
                        line["reasons"] = reasons
                            .iter()
                            .map(|reason| {
                                json!({
                                    "qualifier": reason.qualifier,
                                    "spelled": reason.spelled,
                                    "declared": reason.declared,
                                })
                            })
                            .collect();
                    }
                    line
                })
                .collect();
            json!({"item": "references", "data": {
                "name": declared.name,
                "domain": declared.domain,
                "qualification": declared.qualification.name(),
                "links": links,
            }})
        })
        .collect()
}

fn outcome_name(outcome: &LinkOutcome) -> &'static str {
    match outcome {
        LinkOutcome::Resolved => "resolved",
        LinkOutcome::Unresolved => "unresolved",
        LinkOutcome::Ambiguous => "ambiguous",
        LinkOutcome::UnmetQualifier(_) => "unmet-qualifier",
        _ => "unknown",
    }
}

/// Names indexed by a dense runtime id; an id the plan never assigned
/// stays empty.
fn id_names(n: &Names, ids: impl Iterator<Item = (usize, Sym)>) -> Vec<String> {
    let mut names = Vec::new();
    for (id, sym) in ids {
        if names.len() <= id {
            names.resize(id + 1, String::new());
        }
        names[id] = n.resolve(sym).to_owned();
    }
    names
}

/// Whether closing the scope runs checks: a minimum, an exactly-one
/// group, a required first node, an ordered sequence, or descendant
/// bounds.
fn completes(rules: &ScopeRules) -> bool {
    rules.counters().iter().any(|occurs| occurs.min > 0)
        || !rules.groups().is_empty()
        || rules
            .order_rule()
            .is_some_and(|order| order.first.is_some())
        || rules.sequence().is_some()
        || !rules.descendants().is_empty()
}

fn policy_name(policy: ContextPolicy) -> &'static str {
    match policy {
        ContextPolicy::Fallback => "fallback",
        ContextPolicy::Fixed => "fixed",
        _ => "unknown",
    }
}

/// Spells the plan's numbered slots and namespaces by their declared
/// names.
struct PlanNames<'a> {
    n: &'a Names,
    slots: &'a [String],
    namespaces: &'a [String],
}

impl PlanNames<'_> {
    fn slot(&self, slot: usize) -> &str {
        self.slots.get(slot).map_or("", String::as_str)
    }

    fn effects(&self, steps: &[EffectStep]) -> Vec<serde_json::Value> {
        use serde_json::json;
        let n = self.n;
        steps
            .iter()
            .map(|step| match step {
                EffectStep::Set { slot, from, layer } => {
                    let (key, value) = match from {
                        SetFrom::Field(field) => ("field", json!(n.resolve(*field))),
                        SetFrom::Value(value) => ("value", json!(value)),
                        _ => ("value", serde_json::Value::Null),
                    };
                    json!({
                        "effect": "set",
                        "slot": self.slot(slot.index()),
                        key: value,
                        "layer": layer.map(|l| n.resolve(l)),
                    })
                }
                EffectStep::Remove { slot } => {
                    json!({"effect": "remove", "slot": self.slot(slot.index())})
                }
                EffectStep::Clear { namespace } => json!({
                    "effect": "clear",
                    "namespace": self.namespaces.get(namespace.index()).map_or("", String::as_str),
                }),
                EffectStep::Handler { name } => json!({"effect": "handler", "name": name}),
                _ => json!({"effect": "unknown"}),
            })
            .collect()
    }

    fn deferred(&self, deferred: &DeferredBinding) -> serde_json::Value {
        use serde_json::json;
        match deferred {
            DeferredBinding::Reference { field, target } => json!({
                "deferred": "reference",
                "field": self.n.resolve(*field),
                "target": self.n.resolve(*target),
            }),
            DeferredBinding::Contribution => json!({"deferred": "contribution"}),
            _ => json!({"deferred": "unknown"}),
        }
    }
}

/// The effective view of a compiled definition: its types, nodes,
/// slots, imports and profiles, in that order, each group sorted the
/// way `list` sorts it.
fn effective_lines(def: &Definition) -> Vec<serde_json::Value> {
    use serde_json::json;
    let n = &def.names;
    let entry = |section: &str, entry: serde_json::Value| json!({"item": "effective", "data": {"section": section, "entry": entry}});
    let mut lines = Vec::new();
    for t in list_types(def) {
        let value = json!({"name": t.name, "parameters": t.parameters, "kind": t.kind});
        lines.push(entry("type", value));
    }
    for node in list_nodes(def) {
        let value = json!({
            "name": node.name,
            "parents": node.parents,
            "role": node.role,
            "type": node.ty,
            "profile": node.profile,
        });
        lines.push(entry("node", value));
    }
    let mut slots: Vec<&SlotDecl> = def.slots.iter().collect();
    slots.sort_by_key(|slot| n.resolve(slot.name));
    for slot in slots {
        let value = json!({
            "name": n.resolve(slot.name),
            "type": type_text(n, &slot.ty),
            "namespace": slot.namespace.map(|s| n.resolve(s)),
            "accumulate": slot.accumulate.map(AccumulateOp::name),
            "duplicates": slot.duplicates.name(),
            "layers": slot.layers.iter().map(|l| n.resolve(*l)).collect::<Vec<_>>(),
        });
        lines.push(entry("slot", value));
    }
    let mut imports: Vec<&ImportDecl> = def.imports.iter().collect();
    imports.sort_by_key(|import| (&import.path, import.alias.map(|a| n.resolve(a))));
    for import in imports {
        let value = json!({
            "path": import.path,
            "project": import.project.iter().map(|p| n.resolve(*p)).collect::<Vec<_>>(),
            "repeated": import.repeated,
            "alias": import.alias.map(|a| n.resolve(a)),
        });
        lines.push(entry("import", value));
    }
    for profile in list_profiles(def) {
        let value = json!({
            "name": profile.name,
            "roots": profile.roots,
            "grammar": profile.grammar,
        });
        lines.push(entry("profile", value));
    }
    lines
}

/// A type expression in the definition's own spelling.
fn type_text(n: &Names, ty: &TypeExpr) -> String {
    let apply = |head: &str, args: &[&TypeExpr]| {
        let args: Vec<String> = args.iter().map(|a| type_text(n, a)).collect();
        format!("{head}<{}>", args.join(", "))
    };
    match ty {
        TypeExpr::Named(name) => n.resolve(*name).to_owned(),
        TypeExpr::Instantiation(head, args) => {
            apply(n.resolve(*head), &args.iter().collect::<Vec<_>>())
        }
        TypeExpr::Optional(inner) => apply("optional", &[inner]),
        TypeExpr::Nullable(inner) => apply("nullable", &[inner]),
        TypeExpr::List(inner) => apply("list", &[inner]),
        TypeExpr::Ref(inner) => apply("ref", &[inner]),
        TypeExpr::Map(key, value) => apply("map", &[key, value]),
        TypeExpr::OrderedMap(key, value) => apply("ordered-map", &[key, value]),
        TypeExpr::Union(members) => apply("union", &members.iter().collect::<Vec<_>>()),
        other => format!("{other:?}"),
    }
}

/// Print each value as one JSON line; a failed write fails the run.
fn write_lines(lines: &[serde_json::Value]) -> ExitStatus {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    for line in lines {
        if serde_json::to_writer(&mut out, line).is_err() || out.write_all(b"\n").is_err() {
            return ExitStatus::Failure;
        }
    }
    if out.flush().is_err() {
        return ExitStatus::Failure;
    }
    ExitStatus::Success
}

/// The compile stage: the same load `vet` runs on a dialect or schema
/// target, with complete checking.
fn check(
    target: VetTarget,
    governing: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    anchors: &[(String, PathBuf)],
) -> Result<Definition, Vec<DiagnosticRecord>> {
    match target {
        VetTarget::Schema => load_schema_anchored(
            governing,
            entries,
            extensions,
            CheckScope::Complete,
            anchors,
        ),
        _ => load_dialect(entries, extensions, CheckScope::Complete),
    }
}

fn write_item(out: &mut impl std::io::Write, item: &TypedItem<Item>) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
