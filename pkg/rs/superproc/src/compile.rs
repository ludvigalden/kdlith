//! The self-hosted compiler entry point: definition documents load
//! through this processor's own typed reader, and the engine compiles
//! what the reader hands it (see docs/dialect.md for the seed cycle).

use std::path::Path;

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Severity};
use kdlith_engine::{
    CheckScope, ChildRuleInput, ConstraintInput, Construct, DeferralInput, Definition,
    DefinitionInput, DocumentInput, DocumentRuleInput, EffectGroupInput, EffectInput, Extensions,
    FieldBindingInput, FieldInput, FormatInput, ImportInput, Located, MemberInput, NodeInput,
    ParsedDocument, SlotInput, SourceInput, TypeInput, TypedItem, ValidatorInput, VariantInput,
    compile_dialect, compile_dialect_input, compile_schema_input,
};
use kdlith_runtime::Repeated;

use crate::processors::{Item, reader};
use crate::types::{
    Constraint, Deferral, DocumentRule, Effect, EnumType, FieldBinding, MapType, NodeDef,
    ObjectType, Source, StringType, UnionType,
};

/// The canonical dialect's contributing documents, embedded so the
/// seed does not depend on the working tree's files.
const CANONICAL: [(&str, &str); 4] = [
    (
        "types.kdl",
        include_str!("../../../kdl/schemas/dialect/types.kdl"),
    ),
    (
        "bindings.kdl",
        include_str!("../../../kdl/schemas/dialect/bindings.kdl"),
    ),
    (
        "context.kdl",
        include_str!("../../../kdl/schemas/dialect/context.kdl"),
    ),
    (
        "documents.kdl",
        include_str!("../../../kdl/schemas/dialect/documents.kdl"),
    ),
];

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
    let input = gather(&dialect, entries, scope)?;
    compile_dialect_input(&input, extensions, scope)
}

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
    let input = gather(dialect, entries, scope)?;
    compile_schema_input(dialect, &input, extensions, scope)
}

/// Read `entries` through the typed reader over `dialect`, collecting
/// the item stream into compiler input. An error diagnostic, or a
/// broken reader contract, fails the load with the records collected.
fn gather(
    dialect: &Definition,
    entries: &[&Path],
    scope: CheckScope,
) -> Result<DefinitionInput, Vec<DiagnosticRecord>> {
    let typed = reader(dialect)?;
    let mut input = DefinitionInput::default();
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut sink = |item: TypedItem<Item>| take(item, &mut input, &mut records);
    let read = match scope {
        CheckScope::Complete => typed.read(entries, &mut sink),
        CheckScope::Local => typed.read_local(entries, &mut sink),
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
    Ok(input)
}

/// One item of the stream, into compiler input.
fn take(item: TypedItem<Item>, input: &mut DefinitionInput, records: &mut Vec<DiagnosticRecord>) {
    match item {
        TypedItem::Document { path, text, .. } => {
            input.documents.push(DocumentInput {
                path,
                text,
                constructs: Vec::new(),
            });
        }
        TypedItem::Declaration { value, span, .. } => {
            push(input, records, declaration(value), span);
        }
        TypedItem::Record { value, span, .. } => {
            if let Item::DocumentRule(rule) = value {
                let DocumentRule { first, following } = rule;
                push(
                    input,
                    records,
                    Some(Construct::Rule(DocumentRuleInput {
                        first: first.map(|first| {
                            first
                                .exactly_one
                                .iter()
                                .map(|name| name.as_str().to_owned())
                                .collect()
                        }),
                        following: following
                            .iter()
                            .map(|name| name.as_str().to_owned())
                            .collect(),
                    })),
                    span,
                );
            }
        }
        TypedItem::Import(request) => {
            let span = request.site.span.byte_span();
            push(
                input,
                records,
                Some(Construct::Import(ImportInput {
                    path: request.expression,
                    project: request.project,
                    repeated: repeated_name(request.repeated).to_owned(),
                })),
                span,
            );
        }
        TypedItem::Diagnostic(record) => records.push(*record),
        // A scope's completion carries no construct of its own.
        TypedItem::ScopeComplete(_) => {}
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

/// Append one located construct to the document being read.
fn push(
    input: &mut DefinitionInput,
    records: &mut Vec<DiagnosticRecord>,
    construct: Option<Construct>,
    span: ByteSpan,
) {
    let Some(construct) = construct else { return };
    let Some(document) = input.documents.last_mut() else {
        records.push(broken_contract("a construct arrived before its document"));
        return;
    };
    document.constructs.push(Located {
        item: construct,
        span,
    });
}

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

/// A declaration's typed value, into the construct it declares.
fn declaration(value: Item) -> Option<Construct> {
    let construct = match value {
        Item::StringType(StringType {
            name,
            format,
            width,
            pattern,
            validator,
            validator_version,
            min_length,
        }) => Construct::Type(TypeInput::String {
            name: name.as_str().to_owned(),
            format: format.map(|format| FormatInput {
                format: format.member().to_owned(),
                width,
            }),
            pattern,
            validator: validator.map(|validator| ValidatorInput {
                name: validator.as_str().to_owned(),
                version: validator_version,
            }),
            min_length,
        }),
        Item::EnumType(EnumType { name, members }) => Construct::Type(TypeInput::Enum {
            name: name.as_str().to_owned(),
            members: members
                .iter()
                .map(|(name, member)| MemberInput {
                    name: name.as_str().to_owned(),
                    description: member.description.clone(),
                })
                .collect(),
        }),
        Item::ObjectType(ObjectType {
            name,
            fields,
            constraints,
        }) => Construct::Type(TypeInput::Object {
            name: name.as_str().to_owned(),
            fields: fields
                .iter()
                .map(|(name, field)| FieldInput {
                    name: name.as_str().to_owned(),
                    ty: field.ty.as_str().to_owned(),
                    description: field.description.clone(),
                    documented_default: field.documented_default.clone(),
                    fallback: field.fallback.clone(),
                })
                .collect(),
            constraints: constraints.iter().map(constraint).collect(),
        }),
        Item::UnionType(UnionType { name, variants }) => Construct::Type(TypeInput::Union {
            name: name.as_str().to_owned(),
            variants: variants
                .iter()
                .map(|(name, variant)| VariantInput {
                    discriminator: name.as_str().to_owned(),
                    ty: variant.ty.as_str().to_owned(),
                })
                .collect(),
        }),
        Item::MapType(MapType {
            name,
            key_type,
            value_type,
        }) => Construct::Type(TypeInput::Map {
            name: name.as_str().to_owned(),
            key: key_type.as_str().to_owned(),
            value: value_type.as_str().to_owned(),
        }),
        Item::NodeDef(node) => Construct::Node(node_input(node)),
        Item::SlotDef(slot) => Construct::Slot(SlotInput {
            name: slot.name.as_str().to_owned(),
            ty: slot.ty.as_str().to_owned(),
            namespace: slot.namespace.as_ref().map(|ns| ns.as_str().to_owned()),
            description: slot.description,
        }),
        Item::Deferral(Deferral { mechanism, note }) => Construct::Deferral(DeferralInput {
            mechanism: mechanism.as_str().to_owned(),
            note,
        }),
        // Records reach `take` as `TypedItem::Record`.
        Item::DocumentRule(_) => return None,
    };
    Some(construct)
}

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
    }
}

fn node_input(node: NodeDef) -> NodeInput {
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
        fields: node
            .fields
            .iter()
            .map(|(name, binding)| field_binding(name.as_str().to_owned(), binding))
            .collect(),
        children: node
            .children
            .iter()
            .map(|(name, rule)| ChildRuleInput {
                name: name.as_str().to_owned(),
                min: rule.min_occurs,
                max: rule.max_occurs,
            })
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
    }
}

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

fn source(input: &Source) -> SourceInput {
    match input {
        Source::Argument(argument) => SourceInput::Argument(argument.index),
        Source::Args(_) => SourceInput::Args,
        Source::Property(property) => SourceInput::Property(property.name.as_str().to_owned()),
        Source::Child(child) => SourceInput::Child {
            name: child.name.as_str().to_owned(),
            sources: child.sources.iter().map(&source).collect(),
        },
        Source::Children(children) => SourceInput::Children {
            name: children.name.as_str().to_owned(),
            sources: children.sources.iter().map(&source).collect(),
        },
        Source::Entries(entries) => SourceInput::Entries {
            node: entries.node.as_str().to_owned(),
            key: entries.key.clone(),
            value: entries.value.as_str().to_owned(),
        },
        Source::Context(context) => SourceInput::Context {
            slot: context.slot.as_str().to_owned(),
            policy: context.policy.map(|policy| policy.member().to_owned()),
        },
    }
}

fn effect(effect: &Effect) -> EffectInput {
    match effect {
        Effect::Set(set) => EffectInput::Set {
            slot: set.slot.as_str().to_owned(),
            field: set.field.as_ref().map(|field| field.as_str().to_owned()),
            value: set.value.clone(),
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

fn repeated_name(repeated: Repeated) -> &'static str {
    match repeated {
        Repeated::Reject => "reject",
        Repeated::Coalesce => "coalesce",
        Repeated::Instantiate => "instantiate",
        _ => "reject",
    }
}
