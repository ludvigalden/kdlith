//! The canonical definition vocabulary's pinned emission content: the
//! prose and compile glue the compiled surface does not carry.

/// The type doc comments of the canonical vocabulary, by type name.
pub(crate) fn type_doc(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "TypeExpression" => &["A type-description grammar inside a quoted string."],
        "Sensitivity" => &[
            "The sensitivity a logical field may declare; metadata that every",
            "output surface enforces through an explicit policy.",
        ],
        "TypeDef" => &["A type declaration, discriminated by the declaring node's name."],
        "CodecType" => &[
            "A pattern codec declaration: one whole-string match, the named",
            "captures it types, and the output type a decoded value carries.",
        ],
        "Capture" => &["The capture's name is the key of the enclosing ordered map."],
        "IncludeRef" => &["An include directive among a declaring container's like members."],
        "Bound" => &[
            "An inclusive range over a numeric field's value; either end may be",
            "absent. A list field is bounded per element.",
        ],
        "Role" => &["What the processing session does with a node's bound result."],
        "DependentBinding" => &[
            "A dependent payload binding: how a node's remaining payload binds",
            "against the declaration a field of the same node references.",
        ],
        "DependentTarget" => &[
            "The field of the same node holding the reference, and the field of",
            "the referenced declaration's type holding the expected payload type.",
        ],
        "BindingCodec" => &[
            "The registered binding codecs a dependent value may name; an unknown",
            "codec fails closed, and a known one the reading processor does not",
            "install compiles and binds as a pending typed-binding item.",
        ],
        "CaseDispatch" => &[
            "A case dispatch: the logical type a node binds as, selected by",
            "property value, top-down with first match winning.",
        ],
        "CaseRule" => &[
            "One case: when the named property is present and scalar-equal to",
            "equals, the node binds as the type.",
        ],
        "Otherwise" => &[
            "The fallback of a case dispatch: the type a node binds as when no",
            "case matches.",
        ],
        "Source" => &["Where a field's value comes from."],
        "ExpandSource" => &[
            "Declaration-group expansion: the entries of the named keyed member",
            "of the named declaration splice into the collection being bound.",
        ],
        "AccumulateOp" => &[
            "How an accumulating effect combines with the slot's incoming",
            "value: append concatenates lists; merge combines maps key-wise.",
        ],
        "SlotDuplicates" => &[
            "How a set effect reaching an occupied slot is treated; the",
            "fail-closed default rejects.",
        ],
        "MergeDuplicates" => &[
            "How merge treats a key the held map already carries; the",
            "fail-closed default rejects.",
        ],
        "SourceContextDef" => &[
            "Fields derived from source metadata before the document's first",
            "semantic node; the mode is explicit, with no default.",
        ],
        "PathStemSource" => &["The path-stem source-metadata provider."],
        "SnapshotIndexEntry" => &[
            "One entry of the index the engine writes while scanning, so a",
            "record can be resolved later by restoring its context snapshot.",
        ],
        "ProfileDef" => &[
            "A named selection of root types; the invocation selects it by",
            "name, else by the first matching declared rule, else as the",
            "schema's default, never from document content.",
        ],
        "ProfileMatch" => &[
            "The match rules that select a profile from the source path as",
            "the invocation spells it; the first matching rule wins.",
        ],
        "MatchRule" => &["One match rule, discriminated by its node name."],
        "MatchText" => &["The text a match rule compares; never empty."],
        "PathSuffixMatch" => &["Matches when the source path ends with the text."],
        "PathPrefixMatch" => &["Matches when the source path starts with the text."],
        "FileNameMatch" => &[
            "Matches when the source path's final component, extension",
            "included, equals the text.",
        ],
        "DescendantRule" => &[
            "An occurrence bound over the declaring node's whole subtree,",
            "checked when the declaring scope completes.",
        ],
        "SequenceRule" => &[
            "An ordered sequence the declaring node's immediate children must",
            "match; repetition stays with the step's occurrence bounds.",
        ],
        "DiscoveryDef" => &[
            "The restricted path-expansion grammar import expressions are",
            "evaluated under; anchors are host-supplied typed paths.",
        ],
        "AnchorDef" => &["The anchor's name is the key of the enclosing ordered map."],
        "PatternsRule" => &[
            "The pattern syntax a discovery grammar admits; absent means not",
            "admitted.",
        ],
        "DiscoveryOrder" => &[
            "The only defined ordering policy: matched sources are ordered by",
            "their portable path bytes.",
        ],
        "IdentityBlock" => &[
            "The identity block of a declaration: the domain its qualified",
            "symbol belongs to and how the symbol composes from bound fields.",
        ],
        "ReferenceDef" => &[
            "A named reference type: its target, codec, lookup domain,",
            "qualification policy, and declared qualifiers.",
        ],
        "ResolutionPolicyDef" => &[
            "The contribution-resolution policy: the registered resolver and",
            "the descriptors of how it applies contributions across layers.",
        ],
        "DefaultSpec" => &[
            "A field's fallback: a literal value that supplies an absent",
            "field, or a computed descriptor the processor never computes.",
        ],
        "ComputedDefault" => &[
            "A computed default: computation belongs to the owning domain",
            "process, never to the processor.",
        ],
        "SecretProvider" => &[
            "A secret provider: descriptor data only, never executed and",
            "never reinterpreted as a shell command string.",
        ],
        _ => return None,
    })
}

/// The inline field comments of the canonical vocabulary, by type and
/// field key.
pub(crate) fn field_comment(ty: &str, field: &str) -> Option<&'static [&'static str]> {
    Some(match (ty, field) {
        ("SlotDef", "accumulate") => &[
            "The slot's accumulation policy: how an accumulating effect",
            "combines with the value already held.",
        ],
        ("SlotDef", "duplicates") => &[
            "How a second value reaching a slot that already holds one is",
            "treated when the slot does not accumulate.",
        ],
        ("SlotDef", "layers") => &["The layer order of a layered slot, highest precedence first."],
        ("AccumulationPolicy", "duplicates") => &["How merge treats a key contributed twice."],
        ("SetEffect", "layer") => &["The layer a set on a layered slot names."],
        ("ProfileDef", "extends") => &[
            "The profile this one extends: roots and match rules compose with",
            "the parent's, and an own grammar overrides the inherited one.",
        ],
        ("ProfileDef", "default") => &[
            "Marks the schema's default profile; at most one profile is the",
            "default.",
        ],
        ("ImportRequest", "alias") => &[
            "The imported declarations are reachable through the alias",
            "namespace and not by their unqualified names.",
        ],
        ("QualifierRule", "required") => {
            &["A declared qualifier is required unless it says otherwise."]
        }
        _ => return None,
    })
}

/// The header comment of a `types/<doc>.rs` file, when the document is
/// one the canonical vocabulary describes.
pub(crate) fn types_header(file_name: &str) -> Option<&'static [&'static str]> {
    Some(match file_name {
        "types.kdl" => &[
            "Logical types contributed by `types.kdl`: names, type expressions,",
            "built-in scalars and formats, and the type declarations themselves.",
        ],
        "bindings.kdl" => &[
            "Logical types contributed by `bindings.kdl`: roles, node",
            "declarations, field bindings and their sources, collision and",
            "duplicate-key policies, and child cardinality.",
        ],
        "context.kdl" => &[
            "Logical types contributed by `context.kdl`: typed slots, effect",
            "groups and targets, effects, context policies, source context,",
            "accumulating context, and context snapshots.",
        ],
        "documents.kdl" => &[
            "Logical types contributed by `documents.kdl`: definition roots,",
            "structural document rules, source imports, and the documents",
            "surface — profiles, sequences, discovery, the symbol catalog, and",
            "the descriptor types.",
        ],
        _ => return None,
    })
}

/// The header comment of a `bindings/<doc>.rs` file.
pub(crate) fn bindings_header(file_name: &str) -> Option<&'static [&'static str]> {
    Some(match file_name {
        "types.kdl" => &[
            "Node declarations contributed by `types.kdl`: how type declarations",
            "are spelled in a definition.",
        ],
        "bindings.kdl" => &[
            "Node declarations contributed by `bindings.kdl`: node declarations,",
            "field bindings, binding sources, and child rules.",
        ],
        "context.kdl" => &[
            "Node declarations contributed by `context.kdl`: context containers,",
            "slots, effect groups, and effects.",
        ],
        "documents.kdl" => &[
            "Node declarations contributed by `documents.kdl`: definition roots,",
            "containers, structural rules, and imports.",
        ],
        _ => return None,
    })
}

/// The header comment of a `processors/<doc>.rs` file.
pub(crate) fn processors_header(file_name: &str) -> Option<&'static [&'static str]> {
    Some(match file_name {
        "types.kdl" => &["Handlers for the items `types.kdl` declares: type declarations."],
        "bindings.kdl" => &["Handlers for the items `bindings.kdl` declares: node declarations."],
        "context.kdl" => &["Handlers for the items `context.kdl` declares: context slots."],
        "documents.kdl" => &[
            "Handlers for the items `documents.kdl` declares: document rule",
            "records and the declarations the documents surface adds.",
        ],
        _ => return None,
    })
}

/// One item type's compile glue: the `declaration` match arm, its
/// engine inputs, and the crate types it names.
pub(crate) struct Glue {
    /// The `declaration` arm, verbatim, at its match indentation.
    pub arm: &'static str,
    /// The `take` arm for a record item, when the item is one.
    pub record_arm: Option<&'static str>,
    /// Engine names the arm and its helpers `use`.
    pub engine_inputs: &'static [&'static str],
    /// Crate type names the arm and its helpers `use`.
    pub types: &'static [&'static str],
}

/// The record items in the order `take` spells them.
pub(crate) const RECORD_ORDER: [&str; 2] = ["DocumentRule", "SourceContextDef"];

pub(crate) fn glue(name: &str) -> Option<Glue> {
    let glue = match name {
        "StringType" => Glue {
            arm: STRING_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput", "FormatInput", "ValidatorInput"],
            types: &["StringType"],
        },
        "EnumType" => Glue {
            arm: ENUM_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput", "MemberInput"],
            types: &["EnumType"],
        },
        "ObjectType" => Glue {
            arm: OBJECT_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput", "FieldInput", "ConstraintInput", "DefaultInput"],
            types: &["ObjectType", "Constraint", "DefaultSpec"],
        },
        "UnionType" => Glue {
            arm: UNION_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput", "VariantInput"],
            types: &["UnionType"],
        },
        "MapType" => Glue {
            arm: MAP_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput"],
            types: &["MapType"],
        },
        "CodecType" => Glue {
            arm: CODEC_TYPE_ARM,
            record_arm: None,
            engine_inputs: &["TypeInput", "CaptureInput"],
            types: &["CodecType"],
        },
        "NodeDef" => Glue {
            arm: NODE_DEF_ARM,
            record_arm: None,
            engine_inputs: &[
                "NodeInput",
                "DependentInput",
                "CaseInput",
                "ChildRuleInput",
                "DescendantRuleInput",
                "EffectGroupInput",
                "EffectInput",
                "IdentityInput",
                "SequenceInput",
                "StepInput",
                "FieldBindingInput",
                "SourceInput",
                "NodeAnnotationInput",
            ],
            types: &["NodeDef", "FieldBinding", "Source", "Effect"],
        },
        "AnnotationDef" => Glue {
            arm: ANNOTATION_DEF_ARM,
            record_arm: None,
            engine_inputs: &["AnnotationInput"],
            types: &["AnnotationDef"],
        },
        "SlotDef" => Glue {
            arm: SLOT_DEF_ARM,
            record_arm: None,
            engine_inputs: &["SlotInput"],
            types: &[],
        },
        "ProfileDef" => Glue {
            arm: PROFILE_DEF_ARM,
            record_arm: None,
            engine_inputs: &["ProfileInput", "MatchRuleInput"],
            types: &["ProfileDef", "MatchRule"],
        },
        "DiscoveryDef" => Glue {
            arm: DISCOVERY_DEF_ARM,
            record_arm: None,
            engine_inputs: &["DiscoveryInput", "AnchorInput"],
            types: &["DiscoveryDef"],
        },
        "ReferenceDef" => Glue {
            arm: REFERENCE_DEF_ARM,
            record_arm: None,
            engine_inputs: &["ReferenceInput", "QualifierInput"],
            types: &["ReferenceDef"],
        },
        "ResolutionPolicyDef" => Glue {
            arm: RESOLUTION_POLICY_DEF_ARM,
            record_arm: None,
            engine_inputs: &["ResolutionInput"],
            types: &["ResolutionPolicyDef"],
        },
        "SecretProvider" => Glue {
            arm: SECRET_PROVIDER_ARM,
            record_arm: None,
            engine_inputs: &["ProviderInput"],
            types: &["SecretProvider"],
        },
        "DocumentRule" => Glue {
            arm: "",
            record_arm: Some(DOCUMENT_RULE_TAKE_ARM),
            engine_inputs: &["DocumentRuleInput"],
            types: &["DocumentRule"],
        },
        "SourceContextDef" => Glue {
            arm: "",
            record_arm: Some(SOURCE_CONTEXT_DEF_TAKE_ARM),
            engine_inputs: &["SourceContextInput", "SourceFieldInput"],
            types: &[],
        },
        _ => return None,
    };
    Some(glue)
}

const STRING_TYPE_ARM: &str = r##"        Item::StringType(StringType {
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
"##;

const ENUM_TYPE_ARM: &str = r##"        Item::EnumType(EnumType {
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
"##;

const OBJECT_TYPE_ARM: &str = r##"        Item::ObjectType(ObjectType {
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
"##;

const UNION_TYPE_ARM: &str = r##"        Item::UnionType(UnionType {
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
"##;

const MAP_TYPE_ARM: &str = r##"        Item::MapType(MapType {
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
"##;

const CODEC_TYPE_ARM: &str = r##"        Item::CodecType(CodecType {
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
"##;

const NODE_DEF_ARM: &str = r##"        Item::NodeDef(node) => Construct::Node(Box::new(node_input(node))),
"##;

const ANNOTATION_DEF_ARM: &str = r##"        Item::AnnotationDef(AnnotationDef { name, ty, required }) => {
            Construct::Annotation(AnnotationInput {
                name: name.as_str().to_owned(),
                ty: ty.as_str().to_owned(),
                required: required.is_some(),
            })
        }
"##;

const SLOT_DEF_ARM: &str = r##"        Item::SlotDef(slot) => Construct::Slot(SlotInput {
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
"##;

const PROFILE_DEF_ARM: &str = r##"        Item::ProfileDef(ProfileDef {
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
"##;

const DISCOVERY_DEF_ARM: &str = r##"        Item::DiscoveryDef(DiscoveryDef {
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
"##;

const REFERENCE_DEF_ARM: &str = r##"        Item::ReferenceDef(ReferenceDef {
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
"##;

const RESOLUTION_POLICY_DEF_ARM: &str = r##"        Item::ResolutionPolicyDef(ResolutionPolicyDef {
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
"##;

const SECRET_PROVIDER_ARM: &str = r##"        Item::SecretProvider(SecretProvider { id, ttl, command }) => {
            Construct::Provider(ProviderInput {
                id: id.as_str().to_owned(),
                ttl,
                command,
            })
        }
"##;

const DOCUMENT_RULE_TAKE_ARM: &str = r##"            Item::DocumentRule(rule) => {
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
"##;

const SOURCE_CONTEXT_DEF_TAKE_ARM: &str = r##"            // A source-context block is a record: a statement about the
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
"##;
