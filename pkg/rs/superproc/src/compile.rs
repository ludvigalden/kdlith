//! The self-hosted compiler entry point: definition documents load
//! through this processor's own typed reader, and the engine compiles
//! what the reader hands it (see docs/dialect.md for the seed cycle).

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Severity};
use kdlith_engine::{
    AnchorInput, AnnotationInput, CaptureInput, CaseInput, CheckScope, ChildRuleInput,
    ConditionInput, ConstraintInput, Construct, DefaultInput, Definition, DefinitionInput,
    DependentInput, DescendantRuleInput, DiscoveryInput, DocumentInput, DocumentRuleInput,
    EffectGroupInput, EffectInput, Extensions, FieldBindingInput, FieldInput, FormatInput,
    GroupInput, IdentityInput, ImportInput, Located, MatchRuleInput, MemberInput,
    NodeAnnotationInput, NodeInput, ParsedDocument, ProfileInput, ProviderInput, QualifierInput,
    ReferenceInput, ResolutionInput, SequenceInput, SlotInput, SourceContextInput,
    SourceFieldInput, SourceInput, StepInput, TypeInput, TypedItem, ValidatorInput, VariantInput,
    compile_dialect, compile_dialect_input, compile_schema_input,
};
use kdlith_runtime::Repeated;

use crate::processors::{Item, reader};
use crate::types::{
    AnnotationDef, ChildRule, CodecType, Condition, Constraint, DefaultSpec, DiscoveryDef,
    DocumentRule, Effect, EnumType, FieldBinding, MapType, MatchRule, NodeDef, NodeGroupDef,
    ObjectType, ProfileDef, ReferenceDef, ResolutionPolicyDef, SecretProvider, Source, StringType,
    UnionType,
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
    let input = gather(&dialect, Entries::Paths(entries), scope, &[])?;
    compile_dialect_input(&input, extensions, scope)
}

/// Load a dialect definition as [`load_dialect`] does, over documents
/// held in memory instead of read from disk. An import inside an
/// in-memory document has no directory to resolve against and fails
/// closed (`KDLITH-IMP001`).
pub fn load_dialect_sources(
    sources: &[kdlith_engine::Source],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let dialect = seed(extensions)?;
    let input = gather(&dialect, Entries::Sources(sources), scope, &[])?;
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
    load_schema_anchored(dialect, entries, extensions, scope, &[])
}

/// Load a schema definition as [`load_schema`] does, expanding the
/// documents' grammar-admitted import expressions against the
/// host-supplied `anchors`, under the grammar the schema root binds.
pub fn load_schema_anchored(
    dialect: &Definition,
    entries: &[&Path],
    extensions: &Extensions,
    scope: CheckScope,
    anchors: &[(String, PathBuf)],
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let input = gather(dialect, Entries::Paths(entries), scope, anchors)?;
    compile_schema_input(dialect, &input, extensions, scope)
}

/// Load a schema definition as [`load_schema`] does, over documents
/// held in memory instead of read from disk. An import inside an
/// in-memory document has no directory to resolve against and fails
/// closed (`KDLITH-IMP001`).
pub fn load_schema_sources(
    dialect: &Definition,
    sources: &[kdlith_engine::Source],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let input = gather(dialect, Entries::Sources(sources), scope, &[])?;
    compile_schema_input(dialect, &input, extensions, scope)
}

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
    anchors: &[(String, PathBuf)],
) -> Result<DefinitionInput, Vec<DiagnosticRecord>> {
    let typed = reader(dialect)?.with_anchors(anchors);
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
    Ok(input.definition)
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
            Item::DocumentRule(rule) => {
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
            // A source-context block is a record: a statement about the
            // documents a definition reads, never referenced by name.
            Item::SourceContextDef(block) => push(
                input,
                records,
                Some(Construct::SourceContext(SourceContextInput {
                    mode: block.mode.member().to_owned(),
                    fields: block
                        .fields
                        .iter()
                        .map(|(name, field)| SourceFieldInput {
                            name: name.as_str().to_owned(),
                            ty: field.ty.as_str().to_owned(),
                            sources: field
                                .sources
                                .iter()
                                .map(|_| "path-stem".to_owned())
                                .collect(),
                        })
                        .collect(),
                })),
                span,
            ),
            // A record of any other type is a reader contract this loader
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
                    alias: request.alias,
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
            parameters,
            format,
            width,
            pattern,
            validator,
            validator_version,
            min_length,
        }) => Construct::Type(TypeInput::String {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
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
        Item::EnumType(EnumType {
            name,
            parameters,
            members,
        }) => Construct::Type(TypeInput::Enum {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
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
            parameters,
            fields,
            constraints,
        }) => Construct::Type(TypeInput::Object {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
            fields: fields
                .iter()
                .map(|(name, field)| FieldInput {
                    name: name.as_str().to_owned(),
                    ty: field.ty.as_str().to_owned(),
                    description: field.description.clone(),
                    documented_default: field.documented_default.clone(),
                    // The bound fallback spells the default: the literal
                    // alternative supplies an absent value; the computed
                    // one is a descriptor the processor never computes.
                    default: field.fallback.as_ref().map(default),
                    sensitivity: field.sensitivity.map(|s| s.member().to_owned()),
                    merge: field.merge.map(|m| m.member().to_owned()),
                })
                .collect(),
            constraints: constraints.iter().map(constraint).collect(),
        }),
        Item::UnionType(UnionType {
            name,
            parameters,
            variants,
        }) => Construct::Type(TypeInput::Union {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
            variants: variants
                .iter()
                .map(|(name, variant)| VariantInput {
                    discriminator: name.as_str().to_owned(),
                    ty: variant.ty.as_str().to_owned(),
                    prefix: variant.prefix.clone(),
                })
                .collect(),
        }),
        Item::MapType(MapType {
            name,
            parameters,
            key_type,
            value_type,
        }) => Construct::Type(TypeInput::Map {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
            key: key_type.as_str().to_owned(),
            value: value_type.as_str().to_owned(),
        }),
        Item::CodecType(CodecType {
            name,
            parameters,
            output,
            r#match: pattern,
            captures,
            namespace,
        }) => Construct::Type(TypeInput::Codec {
            name: name.as_str().to_owned(),
            parameters: parameters
                .iter()
                .flatten()
                .map(|parameter| parameter.as_str().to_owned())
                .collect(),
            output: output.as_str().to_owned(),
            pattern,
            captures: captures
                .iter()
                .map(|(name, capture)| CaptureInput {
                    name: name.as_str().to_owned(),
                    ty: capture.ty.as_str().to_owned(),
                })
                .collect(),
            namespace: namespace.map(|namespace| namespace.as_str().to_owned()),
        }),
        Item::NodeDef(node) => Construct::Node(Box::new(node_input(node))),
        Item::AnnotationDef(AnnotationDef { name, ty, required }) => {
            Construct::Annotation(AnnotationInput {
                name: name.as_str().to_owned(),
                ty: ty.as_str().to_owned(),
                required: required.is_some(),
            })
        }
        Item::NodeGroupDef(NodeGroupDef { name, nodes }) => Construct::Group(GroupInput {
            name: name.as_str().to_owned(),
            nodes: nodes.iter().map(|node| node.as_str().to_owned()).collect(),
        }),
        Item::SlotDef(slot) => Construct::Slot(SlotInput {
            name: slot.name.as_str().to_owned(),
            ty: slot.ty.as_str().to_owned(),
            namespace: slot.namespace.as_ref().map(|ns| ns.as_str().to_owned()),
            description: slot.description,
            accumulate: slot
                .accumulate
                .as_ref()
                .map(|policy| policy.op.member().to_owned()),
            accumulate_duplicates: slot
                .accumulate
                .as_ref()
                .map(|policy| policy.duplicates.member().to_owned()),
            duplicates: Some(slot.duplicates.member().to_owned()),
            layers: slot.layers.as_ref().map(|layers| {
                layers
                    .iter()
                    .map(|layer| layer.as_str().to_owned())
                    .collect()
            }),
        }),
        Item::ProfileDef(ProfileDef {
            name,
            extends,
            default,
            roots,
            grammar,
            r#match,
        }) => Construct::Profile(ProfileInput {
            name: name.as_str().to_owned(),
            extends: extends.map(|parent| parent.as_str().to_owned()),
            default,
            roots: roots.map(|roots| {
                roots
                    .allow
                    .iter()
                    .map(|ty| ty.as_str().to_owned())
                    .collect()
            }),
            grammar: grammar.map(|grammar| grammar.as_str().to_owned()),
            rules: r#match.map(|block| {
                block
                    .rules
                    .iter()
                    .map(|rule| {
                        let (kind, text) = match rule {
                            MatchRule::PathSuffix(rule) => ("path-suffix", &rule.text),
                            MatchRule::PathPrefix(rule) => ("path-prefix", &rule.text),
                            MatchRule::FileName(rule) => ("file-name", &rule.text),
                        };
                        MatchRuleInput {
                            kind: kind.to_owned(),
                            text: text.as_str().to_owned(),
                        }
                    })
                    .collect()
            }),
        }),
        Item::DiscoveryDef(DiscoveryDef {
            name,
            anchors,
            patterns,
            order: _,
            allow_empty,
        }) => Construct::Discovery(DiscoveryInput {
            name: name.as_str().to_owned(),
            anchors: anchors
                .iter()
                .map(|(name, anchor)| AnchorInput {
                    name: name.as_str().to_owned(),
                    provider: anchor.provider.as_str().to_owned(),
                })
                .collect(),
            star: patterns
                .as_ref()
                .is_some_and(|rules| rules.star.unwrap_or(false)),
            doublestar: patterns
                .as_ref()
                .is_some_and(|rules| rules.doublestar.unwrap_or(false)),
            braces: patterns
                .as_ref()
                .is_some_and(|rules| rules.braces.unwrap_or(false)),
            allow_empty: allow_empty.unwrap_or(false),
        }),
        Item::ReferenceDef(ReferenceDef {
            name,
            target,
            codec,
            lookup,
            qualification,
            qualifiers,
        }) => Construct::Reference(ReferenceInput {
            name: name.as_str().to_owned(),
            target: target.as_str().to_owned(),
            codec: codec.as_ref().map(|codec| codec.as_str().to_owned()),
            lookup: lookup.as_ref().map(|domain| domain.as_str().to_owned()),
            qualification: qualification.member().to_owned(),
            qualifiers: qualifiers
                .iter()
                .map(|qualifier| QualifierInput {
                    name: qualifier.name.as_str().to_owned(),
                    ty: qualifier.ty.as_str().to_owned(),
                    // A declared qualifier is required unless it says
                    // otherwise.
                    required: qualifier.required.unwrap_or(true),
                })
                .collect(),
        }),
        Item::ResolutionPolicyDef(ResolutionPolicyDef {
            resolver,
            layers,
            selection,
            duplicates,
            missing,
        }) => Construct::Resolution(ResolutionInput {
            resolver: resolver.as_str().to_owned(),
            layers: layers.unwrap_or_default(),
            selection: selection.member().to_owned(),
            duplicates: duplicates.member().to_owned(),
            missing: missing.member().to_owned(),
        }),
        Item::SecretProvider(SecretProvider { id, ttl, command }) => {
            Construct::Provider(ProviderInput {
                id: id.as_str().to_owned(),
                ttl,
                command,
            })
        }
        // Records reach `take` as `TypedItem::Record`.
        Item::DocumentRule(_) | Item::SourceContextDef(_) => return None,
    };
    Some(construct)
}

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

fn repeated_name(repeated: Repeated) -> &'static str {
    match repeated {
        Repeated::Reject => "reject",
        Repeated::Coalesce => "coalesce",
        Repeated::Instantiate => "instantiate",
        _ => "reject",
    }
}
