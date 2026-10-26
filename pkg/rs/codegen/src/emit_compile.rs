//! `src/compile.rs` emission: the definition-reader entry point for a
//! dialect, assembled from the vocabulary catalog, and the load-only
//! entry point for a schema.

use std::fmt::Write as _;

use crate::Error;
use crate::catalog::{self, RECORD_ORDER};
use crate::model::Emission;

/// Engine names the compile skeleton uses regardless of which items the
/// definition declares.
const SKELETON_INPUTS: [&str; 14] = [
    "CheckScope",
    "Construct",
    "Definition",
    "DefinitionInput",
    "DocumentInput",
    "Extensions",
    "ImportInput",
    "Located",
    "ParsedDocument",
    "ReadOptions",
    "TypedItem",
    "compile_dialect",
    "compile_dialect_input",
    "compile_schema_input",
];

impl Emission<'_> {
    pub(crate) fn compile_rs(&self) -> Result<String, Error> {
        match self.kind {
            kdlith_engine::DefinitionKind::Dialect => self.dialect_compile_rs(),
            _ => Ok(schema_compile_rs()),
        }
    }

    /// The definition-reader entry point: seed, load, gather, and the
    /// declaration mapping, assembled from the items the dialect
    /// declares.
    fn dialect_compile_rs(&self) -> Result<String, Error> {
        let glues: Vec<(String, catalog::Glue)> = self
            .items
            .iter()
            .filter(|item| item.declares_variant)
            .map(|item| {
                catalog::glue(&item.ty)
                    .map(|glue| (item.ty.clone(), glue))
                    .ok_or_else(|| Error::UnsupportedItem {
                        name: item.ty.clone(),
                    })
            })
            .collect::<Result<_, _>>()?;

        let mut engine: Vec<String> = SKELETON_INPUTS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let mut types: Vec<String> = Vec::new();
        for (_, glue) in &glues {
            for input in glue.engine_inputs {
                let input = (*input).to_owned();
                if !engine.contains(&input) {
                    engine.push(input);
                }
            }
            for ty in glue.types {
                let ty = (*ty).to_owned();
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
        }
        engine.sort();
        types.sort();

        let mut seed: Vec<(String, String)> = Vec::new();
        for (file_name, _) in self.seed_documents() {
            let include = crate::render::include_path(file_name);
            seed.push((file_name.clone(), include));
        }

        let mut out = String::new();
        out.push_str(
            r##"//! The self-hosted compiler entry point: definition documents load
//! through this processor's own typed reader, and the engine compiles
//! what the reader hands it (see
//! context.internal/reference/40_engine_contract.md for the seed cycle).

use std::path::Path;

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Severity};
"##,
        );
        out.push_str(&use_statement("kdlith_engine", &engine));
        out.push_str(
            r##"use kdlith_runtime::Repeated;

use crate::processors::{Item, reader};
"##,
        );
        out.push_str(&use_statement("crate::types", &types));

        // The seed: a versioned snapshot of every contributing document,
        // embedded through the crate's own `src/dialect/` directory so
        // the published crate carries its seed with it.
        let _ = writeln!(
            out,
            "
/// The canonical dialect's contributing documents, snapshotted at
/// generation time so the published crate carries its own seed.
const CANONICAL: [(&str, &str); {}] = [",
            seed.len()
        );
        for (file_name, include) in &seed {
            let inline = format!("    (\"{file_name}\", include_str!(\"{include}\")),");
            if inline.chars().count() <= 100 {
                out.push_str(&inline);
                out.push('\n');
            } else {
                let _ = write!(
                    out,
                    "    (\n        \"{file_name}\",\n        include_str!(\"{include}\"),\n    ),\n"
                );
            }
        }
        let _ = writeln!(out, "];");
        let _ = writeln!(
            out,
            "
/// The dialect version the snapshotted documents declare.
pub const CANONICAL_VERSION: &str = {:?};",
            self.version
        );

        out.push_str(SEED);
        out.push_str(LOAD_DIALECT);
        out.push_str(LOAD_SCHEMA);
        out.push_str(GATHER);
        out.push_str(&self.take_fn(&glues));
        out.push_str(PUSH);
        out.push_str(BROKEN_CONTRACT);
        out.push_str(&self.declaration_fn(&glues));
        if glues.iter().any(|(ty, _)| ty == "ObjectType") {
            out.push_str(DEFAULT);
            out.push_str(CONSTRAINT);
        }
        if glues.iter().any(|(ty, _)| ty == "NodeDef") {
            out.push_str(NODE_INPUT);
            out.push_str(FIELD_BINDING);
            out.push_str(SOURCE);
            out.push_str(EFFECT);
            out.push_str(CHILD_RULE);
        }
        if glues
            .iter()
            .any(|(ty, _)| ty == "ProfileDef" || ty == "LocationsRule")
        {
            out.push_str(catalog::LOCATIONS_INPUT);
        }
        out.push_str(REPEATED_NAME);

        Ok(format!("{}\n\n{}", crate::RUST_MARKER, out))
    }

    /// `take`, with the record arms the definition's record items
    /// spell, in the catalog's record order.
    fn take_fn(&self, glues: &[(String, catalog::Glue)]) -> String {
        let mut records = String::new();
        for name in RECORD_ORDER {
            if let Some((_, glue)) = glues.iter().find(|(ty, _)| ty == name)
                && let Some(arm) = glue.record_arm
            {
                records.push_str(arm);
            }
        }
        let mut text = String::from(TAKE_HEAD);
        text.push_str(&records);
        text.push_str(TAKE_TAIL);
        text
    }

    /// `declaration`, with one arm per declaration item in item order
    /// and the record catch-all last.
    fn declaration_fn(&self, glues: &[(String, catalog::Glue)]) -> String {
        let declarations: Vec<&(String, catalog::Glue)> = glues
            .iter()
            .filter(|(_, glue)| !glue.arm.is_empty())
            .collect();
        let records: Vec<&str> = RECORD_ORDER
            .iter()
            .filter(|name| glues.iter().any(|(ty, _)| ty == **name))
            .copied()
            .collect();
        let mut text = String::from(DECLARATION_HEAD);
        for (_, glue) in declarations {
            text.push_str(glue.arm);
        }
        if records.is_empty() {
            text.push_str("        _ => return None,\n");
        } else {
            let patterns = records
                .iter()
                .map(|name| format!("Item::{name}(_)"))
                .collect::<Vec<_>>()
                .join(" | ");
            text.push_str(&format!(
                "        // Records reach `take` as `TypedItem::Record`.\n        {patterns} => return None,\n"
            ));
        }
        text.push_str(DECLARATION_TAIL);
        text
    }
}

/// A `use path::{names};` statement, wrapped like the emitter's lists.
fn use_statement(path: &str, names: &[String]) -> String {
    if let [only] = names {
        return format!("use {path}::{only};\n");
    }
    let single = format!("use {}::{{{}}};", path, names.join(", "));
    // rustfmt keeps a one-line `use` list only up to 98 columns.
    if single.len() <= 98 {
        return format!("{single}\n");
    }
    let mut text = format!("use {path}::{{\n");
    let mut buffer = String::new();
    for name in names {
        // The line is the four-space indent plus the buffer; adding a
        // name costs its own width plus ", ". A wrapped line ends in a
        // trailing comma, so 100 is the one length rustfmt declines.
        if !buffer.is_empty() && 4 + buffer.len() + 2 + name.len() + 1 > 99 {
            text.push_str(&format!("    {buffer},\n"));
            buffer.clear();
        }
        if !buffer.is_empty() {
            buffer.push_str(", ");
        }
        buffer.push_str(name);
    }
    if !buffer.is_empty() {
        text.push_str(&format!("    {buffer},\n"));
    }
    text.push_str("};\n");
    text
}

/// The schema-form entry point: load this schema's documents through
/// the reference processor against the canonical dialect.
fn schema_compile_rs() -> String {
    let body = r##"//! The compile entry point: this schema's documents load through the
//! reference processor against the canonical dialect, and the engine
//! compiles what the reader hands it.

use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_engine::{CheckScope, Definition, Extensions, ReadOptions, Source};

/// Load this schema's definition from its entry documents and compile it
/// under the canonical dialect: the definition language every schema is
/// written in.
pub fn load(
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    load_with(entries, extensions, scope, ReadOptions::new())
}

/// Load this schema's definition as [`load`] does, resolving the
/// documents' own import locations under `options`: its project root
/// and the named sources `$name/` locations address.
pub fn load_with(
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
    options: ReadOptions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let dialect = kdlith_superproc::seed(extensions)?;
    kdlith_superproc::load_schema_with(&dialect, entries, extensions, scope, options)
}

/// Load this schema's definition as [`load`] does, over documents held
/// in memory instead of read from disk. An in-memory document belongs
/// to no source, so an import inside it without a `$name/` prefix fails
/// closed (`KDLITH-LOC001`).
pub fn load_sources(
    sources: &[Source],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    load_sources_with(sources, extensions, scope, ReadOptions::new())
}

/// Load this schema's definition as [`load_sources`] does, resolving the
/// documents' `$name/` import locations in the named sources `options`
/// registers, such as files the host embeds.
pub fn load_sources_with(
    sources: &[Source],
    extensions: &Extensions,
    scope: CheckScope,
    options: ReadOptions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let dialect = kdlith_superproc::seed(extensions)?;
    kdlith_superproc::load_schema_sources_with(&dialect, sources, extensions, scope, options)
}
"##;
    format!("{}\n\n{}", crate::RUST_MARKER, body)
}

const SEED: &str = r##"
/// The canonical dialect, compiled from its embedded documents with the
/// engine's seed bootstrap. This is the only definition the bootstrap
/// ever compiles: the reader every other definition flows through is
/// constructed over it.
pub fn seed(extensions: &Extensions) -> Result<Definition, Vec<DiagnosticRecord>> {
    let mut documents = Vec::with_capacity(CANONICAL.len());
    for (path, text) in CANONICAL {
        let parsed = ParsedDocument::parse(path, text).map_err(|record| vec![*record])?;
        documents.push(parsed);
    }
    compile_dialect(&documents, extensions)
}
"##;

const LOAD_DIALECT: &str = r##"
/// Load a dialect definition from its entry documents and compile it.
/// The documents are read through the reference processor against the
/// canonical dialect — the definition language every dialect is written
/// in — so their binding diagnostics are reported before compilation
/// begins.
pub fn load_dialect(
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let dialect = seed(extensions)?;
    let input = gather(&dialect, Entries::Paths(entries), scope, ReadOptions::new())?;
    compile_dialect_input(&input, extensions, scope)
}

/// Load a dialect definition as [`load_dialect`] does, over documents
/// held in memory instead of read from disk. An in-memory document
/// belongs to no source, so an import inside it fails closed
/// (`KDLITH-LOC001`).
pub fn load_dialect_sources(
    sources: &[kdlith_engine::Source],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let dialect = seed(extensions)?;
    let input = gather(
        &dialect,
        Entries::Sources(sources),
        scope,
        ReadOptions::new(),
    )?;
    compile_dialect_input(&input, extensions, scope)
}
"##;

const LOAD_SCHEMA: &str = r##"
/// Load a schema definition from its entry documents and compile it
/// under `dialect`: the documents are read through the reference
/// processor against the governing dialect, so a construct the dialect
/// does not admit is rejected with the binding code any processor
/// binding it reports.
pub fn load_schema(
    dialect: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    load_schema_with(dialect, entries, extensions, scope, ReadOptions::new())
}

/// Load a schema definition as [`load_schema`] does, resolving the
/// documents' own import locations under `options`: its project root
/// and the named sources `$name/` locations address.
pub fn load_schema_with(
    dialect: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
    options: ReadOptions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let input = gather(dialect, Entries::Paths(entries), scope, options)?;
    compile_schema_input(dialect, &input, extensions, scope)
}

/// Load a schema definition as [`load_schema`] does, over documents
/// held in memory instead of read from disk. An in-memory document
/// belongs to no source, so an import inside it without a `$name/`
/// prefix fails closed (`KDLITH-LOC001`).
pub fn load_schema_sources(
    dialect: &Definition,
    sources: &[kdlith_engine::Source],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    load_schema_sources_with(dialect, sources, extensions, scope, ReadOptions::new())
}

/// Load a schema definition as [`load_schema_sources`] does, resolving
/// the documents' `$name/` import locations in the named sources
/// `options` registers, such as files a host embeds.
pub fn load_schema_sources_with(
    dialect: &Definition,
    sources: &[kdlith_engine::Source],
    extensions: &Extensions,
    scope: CheckScope,
    options: ReadOptions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let input = gather(dialect, Entries::Sources(sources), scope, options)?;
    compile_schema_input(dialect, &input, extensions, scope)
}
"##;

const GATHER: &str = r##"
/// The documents a load reads: entry files, or documents held in memory.
#[derive(Clone, Copy)]
enum Entries<'a> {
    Paths(&'a [&'a Path]),
    Sources(&'a [kdlith_engine::Source]),
}

/// Read `entries` through the typed reader over `dialect`, collecting
/// the item stream into compiler input. An error diagnostic, or a
/// broken reader contract, fails the load with the records collected.
fn gather(
    dialect: &Definition,
    entries: Entries<'_>,
    scope: CheckScope,
    options: ReadOptions,
) -> Result<DefinitionInput, Vec<DiagnosticRecord>> {
    let typed = reader(dialect)?.with_options(options);
    let mut input = Gathering::default();
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut sink = |item: TypedItem<Item>| take(item, &mut input, &mut records);
    let read = match (scope, entries) {
        (CheckScope::Complete, Entries::Paths(paths)) => typed.read(paths, &mut sink),
        (CheckScope::Local, Entries::Paths(paths)) => typed.read_local(paths, &mut sink),
        (CheckScope::Complete, Entries::Sources(sources)) => typed.read_sources(sources, &mut sink),
        (CheckScope::Local, Entries::Sources(sources)) => {
            typed.read_sources_local(sources, &mut sink)
        }
        _ => return Err(vec![broken_contract("an unknown checking scope")]),
    };
    let broken = match read {
        Ok(_) => {
            if records
                .iter()
                .any(|record| record.severity == Severity::Error)
            {
                return Err(records);
            }
            None
        }
        Err(error) => Some(error),
    };
    if let Some(error) = broken {
        records.push(broken_contract(&error.to_string()));
        return Err(records);
    }
    // A reading's warnings and notes, such as an overridden source, ride
    // on the definition.
    let mut definition = input.definition;
    definition.notes = records;
    Ok(definition)
}

/// The compiler input being gathered, with the documents still open: an
/// imported document's items interrupt its importer's, which resume once
/// the imported document completes.
#[derive(Default)]
struct Gathering {
    definition: DefinitionInput,
    open: Vec<usize>,
    position: usize,
}
"##;

const TAKE_HEAD: &str = r##"
/// One item of the stream, into compiler input.
fn take(item: TypedItem<Item>, input: &mut Gathering, records: &mut Vec<DiagnosticRecord>) {
    match item {
        TypedItem::Document { path, text, .. } => {
            input.open.push(input.definition.documents.len());
            input.definition.documents.push(DocumentInput {
                path,
                text,
                constructs: Vec::new(),
            });
        }
        TypedItem::Declaration { value, span, .. } => {
            push(input, records, declaration(value), span);
        }
        TypedItem::Record { value, span, .. } => match value {
"##;

const TAKE_TAIL: &str = r##"            // A record of any other type is a reader contract this loader
            // cannot proceed past: the dialect declares no such record.
            _ => records.push(broken_contract(
                "a definition reading yielded an unknown record",
            )),
        },
        TypedItem::Import(request) => {
            let span = request.site.span.byte_span();
            push(
                input,
                records,
                Some(Construct::Import(ImportInput {
                    path: request.expression,
                    project: request.project,
                    repeated: repeated_name(request.repeated).to_owned(),
                    namespace: request.namespace,
                })),
                span,
            );
        }
        // An include among a declaration's like members: the splice
        // point the expansion pass applies once every type is declared.
        TypedItem::Expansion(request) => {
            let span = request.site.span.byte_span();
            let Some(owner) = request.owner else {
                records.push(broken_contract(
                    "a definition reading yielded an include outside a declaration",
                ));
                return;
            };
            push(
                input,
                records,
                Some(Construct::Include {
                    owner,
                    target: request.target.written,
                    at: request.at,
                }),
                span,
            );
        }
        TypedItem::Diagnostic(record) => records.push(*record),
        // A document's completion closes it; a node scope's carries
        // nothing.
        TypedItem::ScopeComplete(completion) => {
            if completion.scope.is_none() {
                input.open.pop();
            }
        }
        // The reading's sources are provenance, not definition input.
        TypedItem::Sources(_) => {}
        // A definition vocabulary declares no contribution nodes; a
        // reader that yields one here has broken its contract.
        TypedItem::Contribution(_) => {
            records.push(broken_contract(
                "a definition reading yielded a contribution",
            ));
        }
        _ => {
            records.push(broken_contract(
                "a definition reading yielded an unknown item",
            ));
        }
    }
}
"##;

const PUSH: &str = r##"
/// Append one located construct to the innermost open document.
fn push(
    input: &mut Gathering,
    records: &mut Vec<DiagnosticRecord>,
    construct: Option<Construct>,
    span: ByteSpan,
) {
    let Some(construct) = construct else { return };
    let open = input.open.last().copied();
    let Some(document) = open.and_then(|doc| input.definition.documents.get_mut(doc)) else {
        records.push(broken_contract("a construct arrived before its document"));
        return;
    };
    document.constructs.push(Located {
        item: construct,
        span,
        position: input.position,
    });
    input.position += 1;
}
"##;

const BROKEN_CONTRACT: &str = r##"
/// The record for a reader contract this loader cannot proceed past.
fn broken_contract(reason: &str) -> DiagnosticRecord {
    const MISMATCH: &str = "KDLITH-GEN003";
    DiagnosticRecord::from_code(MISMATCH)
        .expect("the processor mismatch code is registered and never retired")
        .with_message(format!(
            "the definition reading broke its contract: {reason}"
        ))
        .with_origin("processor")
}
"##;

const DECLARATION_HEAD: &str = r##"
/// A declaration's typed value, into the construct it declares.
fn declaration(value: Item) -> Option<Construct> {
    let construct = match value {
"##;

const DECLARATION_TAIL: &str = r##"    };
    Some(construct)
}
"##;

const DEFAULT: &str = r##"
/// The default a bound fallback spells: literal or computed, never
/// silently dropped — the binder's case dispatch selected one variant.
fn default(spec: &DefaultSpec) -> DefaultInput {
    match spec {
        DefaultSpec::Literal(literal) => DefaultInput {
            computed: false,
            value: Some(literal.value.clone()),
            dependencies: Vec::new(),
        },
        DefaultSpec::Computed(computed) => DefaultInput {
            computed: true,
            value: None,
            dependencies: computed.dependencies.clone(),
        },
    }
}
"##;

const CONSTRAINT: &str = r##"
fn constraint(constraint: &Constraint) -> ConstraintInput {
    match constraint {
        Constraint::AllOrNone(all_or_none) => ConstraintInput::AllOrNone {
            fields: all_or_none
                .fields
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect(),
        },
        Constraint::Requires(requires) => ConstraintInput::Requires {
            field: requires.field.as_str().to_owned(),
            required: requires.required.as_str().to_owned(),
        },
        Constraint::Bound(bound) => ConstraintInput::Bound {
            field: bound.field.as_str().to_owned(),
            min: bound.min,
            max: bound.max,
        },
        Constraint::AtMostOne(at_most_one) => ConstraintInput::AtMostOne {
            fields: at_most_one
                .fields
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect(),
        },
        Constraint::ExactlyOneField(exactly_one_field) => ConstraintInput::ExactlyOneField {
            fields: exactly_one_field
                .fields
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect(),
        },
    }
}
"##;

const NODE_INPUT: &str = r##"
fn node_input(node: NodeDef) -> NodeInput {
    let value = node.value.map(|value| DependentInput {
        target: value.against.target.as_str().to_owned(),
        member: value.against.member.as_str().to_owned(),
        codec: value.codec.member().to_owned(),
    });
    let (cases, otherwise) = match node.cases {
        Some(dispatch) => {
            let otherwise = dispatch
                .otherwise
                .map(|fallback| fallback.ty.as_str().to_owned());
            let cases = dispatch
                .cases
                .iter()
                .map(|rule| CaseInput {
                    property: rule.property.as_str().to_owned(),
                    equals: rule.equals.clone(),
                    ty: rule.ty.as_str().to_owned(),
                })
                .collect();
            (cases, otherwise)
        }
        None => (Vec::new(), None),
    };
    NodeInput {
        name: node.name.as_str().to_owned(),
        role: node.role.as_ref().map(|role| role.member().to_owned()),
        ty: node.ty.as_ref().map(|ty| ty.as_str().to_owned()),
        parent: node
            .parent
            .as_ref()
            .map(|parent| parent.as_str().to_owned()),
        parents: node
            .parents
            .as_ref()
            .map(|parents| parents.iter().map(|p| p.as_str().to_owned()).collect()),
        parent_group: node
            .parent_group
            .as_ref()
            .map(|group| group.as_str().to_owned()),
        root: node.root,
        profile: node
            .profile
            .as_ref()
            .map(|profile| profile.as_str().to_owned()),
        fields: node
            .fields
            .iter()
            .map(|(name, binding)| field_binding(name.as_str().to_owned(), binding))
            .collect(),
        children: node
            .children
            .iter()
            .map(|(name, rule)| child_rule(name.as_str(), rule))
            .collect(),
        children_groups: node
            .children_groups
            .iter()
            .map(|(name, rule)| child_rule(name.as_str(), rule))
            .collect(),
        effects: node
            .effects
            .iter()
            .map(|group| EffectGroupInput {
                target: group.target.member().to_owned(),
                effects: group.effects.iter().map(effect).collect(),
            })
            .collect(),
        groups: node
            .groups
            .iter()
            .map(|group| group.nodes.iter().map(|n| n.as_str().to_owned()).collect())
            .collect(),
        descendants: node
            .descendants
            .iter()
            .map(|rule| DescendantRuleInput {
                name: rule.name.as_str().to_owned(),
                min: rule.min_occurs,
                max: rule.max_occurs,
            })
            .collect(),
        sequence: node.sequence.as_ref().map(|sequence| SequenceInput {
            steps: sequence
                .steps
                .iter()
                .map(|step| StepInput {
                    name: step.name.as_ref().map(|name| name.as_str().to_owned()),
                    alternatives: step
                        .alternatives
                        .as_ref()
                        .map(|alternatives| {
                            alternatives
                                .nodes
                                .iter()
                                .map(|name| name.as_str().to_owned())
                                .collect()
                        })
                        .unwrap_or_default(),
                    min: step.min_occurs,
                    max: step.max_occurs,
                })
                .collect(),
        }),
        value,
        cases,
        otherwise,
        identity: node.identity.as_ref().map(|identity| IdentityInput {
            domain: identity.domain.name.as_str().to_owned(),
            form: identity.compose.form.member().to_owned(),
            fields: identity
                .compose
                .fields
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect(),
        }),
        description: node.description,
        annotations: node
            .annotations
            .iter()
            .flatten()
            .map(|fact| NodeAnnotationInput {
                name: fact.name.as_str().to_owned(),
                value: fact.value.clone(),
            })
            .collect(),
    }
}
"##;

const FIELD_BINDING: &str = r##"
/// One field binding, its shorthand spellings and its `from` sources
/// arriving as the one source list they compile to.
fn field_binding(name: String, binding: &FieldBinding) -> FieldBindingInput {
    let mut sources = Vec::new();
    if let Some(index) = binding.arg {
        sources.push(SourceInput::Argument(index));
    }
    if let Some(property) = &binding.property {
        sources.push(SourceInput::Property(property.as_str().to_owned()));
    }
    if let Some(child) = &binding.child {
        sources.push(SourceInput::Child {
            name: child.as_str().to_owned(),
            sources: Vec::new(),
            compact: false,
        });
    }
    if let Some(slot) = &binding.context {
        sources.push(SourceInput::Context {
            slot: slot.as_str().to_owned(),
            policy: None,
        });
    }
    sources.extend(binding.sources.iter().map(source));
    FieldBindingInput {
        name,
        ty: binding.ty.as_ref().map(|ty| ty.as_str().to_owned()),
        sources,
        collision: binding
            .collision
            .map(|collision| collision.member().to_owned()),
        duplicate_keys: binding
            .duplicate_keys
            .map(|policy| policy.member().to_owned()),
    }
}
"##;

const SOURCE: &str = r##"
fn source(input: &Source) -> SourceInput {
    match input {
        Source::Argument(argument) => SourceInput::Argument(argument.index),
        Source::Args(_) => SourceInput::Args,
        Source::Property(property) => SourceInput::Property(property.name.as_str().to_owned()),
        Source::Child(child) => SourceInput::Child {
            name: child.name.as_str().to_owned(),
            sources: child.sources.iter().map(&source).collect(),
            compact: child.compact.unwrap_or(false),
        },
        Source::Children(children) => SourceInput::Children {
            name: children.name.as_str().to_owned(),
            sources: children.sources.iter().map(&source).collect(),
            compact: children.compact.unwrap_or(false),
        },
        Source::ChildrenGroup(children_group) => SourceInput::ChildrenGroup {
            name: children_group.name.as_str().to_owned(),
        },
        Source::Entries(entries) => SourceInput::Entries {
            node: entries.node.as_str().to_owned(),
            key: entries.key.clone(),
            value: entries.value.as_str().to_owned(),
        },
        Source::Expand(expand) => SourceInput::Expand {
            target: expand.target.as_str().to_owned(),
            member: expand.member.as_str().to_owned(),
        },
        Source::Context(context) => SourceInput::Context {
            slot: context.slot.as_str().to_owned(),
            policy: context.policy.map(|policy| policy.member().to_owned()),
        },
    }
}
"##;

const EFFECT: &str = r##"
fn effect(effect: &Effect) -> EffectInput {
    match effect {
        Effect::Set(set) => EffectInput::Set {
            slot: set.slot.as_str().to_owned(),
            field: set.field.as_ref().map(|field| field.as_str().to_owned()),
            value: set.value.clone(),
            layer: set.layer.as_ref().map(|layer| layer.as_str().to_owned()),
        },
        Effect::Remove(remove) => EffectInput::Remove {
            slot: remove.slot.as_str().to_owned(),
        },
        Effect::Clear(clear) => EffectInput::Clear {
            namespace: clear.namespace.as_str().to_owned(),
        },
        Effect::Handler(handler) => EffectInput::Handler {
            name: handler.name.as_str().to_owned(),
        },
    }
}
"##;

const CHILD_RULE: &str = r##"
/// One children rule with its when branches, in written order.
fn child_rule(name: &str, rule: &ChildRule) -> ChildRuleInput {
    ChildRuleInput {
        name: name.to_owned(),
        min: rule.min_occurs,
        max: rule.max_occurs,
        branches: rule.branches.iter().flatten().map(condition).collect(),
    }
}

fn condition(input: &Condition) -> ConditionInput {
    match input {
        Condition::When(when) => ConditionInput::When {
            property: when.property.as_str().to_owned(),
            equals: when.equals.clone(),
            min: when.min_occurs,
            max: when.max_occurs,
        },
        Condition::WhenAll(group) => ConditionInput::All {
            members: group.members.iter().map(condition).collect(),
            min: group.min_occurs,
            max: group.max_occurs,
        },
        Condition::WhenAny(group) => ConditionInput::Any {
            members: group.members.iter().map(condition).collect(),
            min: group.min_occurs,
            max: group.max_occurs,
        },
    }
}
"##;

const REPEATED_NAME: &str = r##"
fn repeated_name(repeated: Repeated) -> &'static str {
    match repeated {
        Repeated::Reject => "reject",
        Repeated::Coalesce => "coalesce",
        Repeated::Instantiate => "instantiate",
        _ => "reject",
    }
}
"##;
