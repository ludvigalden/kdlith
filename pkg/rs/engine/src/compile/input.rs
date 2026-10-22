//! What a reader hands the compiler: one construct per declaration,
//! already validated by binding, with the span it was written at.

use kdlith_diagnostic::ByteSpan;
use kdlith_model::Value;

/// One definition's documents, as a reader collected them.
#[derive(Clone, Debug, Default)]
pub struct DefinitionInput {
    pub documents: Vec<DocumentInput>,
}

/// One contributing document: its path, its text, and its constructs in
/// stream order. The text locates diagnostics and re-binds for
/// self-description; the constructs are what the document declares.
#[derive(Clone, Debug)]
pub struct DocumentInput {
    pub path: String,
    pub text: String,
    pub constructs: Vec<Located<Construct>>,
}

/// One construct with the span of the node that declared it and its
/// position in the reading: an imported document's constructs stream
/// between the import and the importer's next construct, and the
/// compiler takes constructs in that order whichever document holds them.
#[derive(Clone, Debug)]
pub struct Located<T> {
    pub item: T,
    pub span: ByteSpan,
    pub position: usize,
}

/// One declaration or request a definition document makes.
#[derive(Clone, Debug)]
pub enum Construct {
    Type(TypeInput),
    Node(Box<NodeInput>),
    Slot(SlotInput),
    Rule(DocumentRuleInput),
    Import(ImportInput),
    /// One `include TARGET` child among a declaring node's keyed
    /// members: the splice the complete-checking expansion pass applies
    /// to `owner` after its first `at` like members.
    Include {
        owner: String,
        target: String,
        at: usize,
    },
    Profile(ProfileInput),
    Discovery(DiscoveryInput),
    Reference(ReferenceInput),
    /// A schema-level annotation declaration.
    Annotation(AnnotationInput),
    /// A schema-level node group declaration.
    Group(GroupInput),
    Resolution(ResolutionInput),
    Provider(ProviderInput),
    SourceContext(SourceContextInput),
}

/// A logical type declaration: what `string`, `enum`, `object`,
/// `union`, `map`, and `codec` declare.
#[derive(Clone, Debug)]
pub enum TypeInput {
    String {
        name: String,
        parameters: Vec<String>,
        format: Option<FormatInput>,
        pattern: Option<String>,
        validator: Option<ValidatorInput>,
        min_length: Option<u64>,
    },
    Enum {
        name: String,
        parameters: Vec<String>,
        members: Vec<MemberInput>,
    },
    Object {
        name: String,
        parameters: Vec<String>,
        fields: Vec<FieldInput>,
        constraints: Vec<ConstraintInput>,
    },
    Union {
        name: String,
        parameters: Vec<String>,
        variants: Vec<VariantInput>,
    },
    Map {
        name: String,
        parameters: Vec<String>,
        key: String,
        value: String,
    },
    Codec {
        name: String,
        parameters: Vec<String>,
        output: String,
        pattern: String,
        captures: Vec<CaptureInput>,
        namespace: Option<String>,
    },
}

/// One typed capture group of a codec.
#[derive(Clone, Debug)]
pub struct CaptureInput {
    pub name: String,
    pub ty: String,
}

/// A node declaration's dependent payload binding.
#[derive(Clone, Debug)]
pub struct DependentInput {
    pub target: String,
    pub member: String,
    pub codec: String,
}

/// One case of a case dispatch, as declared.
#[derive(Clone, Debug)]
pub struct CaseInput {
    pub property: String,
    pub equals: Value,
    pub ty: String,
}

/// A built-in string format and its width, as declared.
#[derive(Clone, Debug)]
pub struct FormatInput {
    pub format: String,
    pub width: Option<u64>,
}

/// A registered validator a string type names.
#[derive(Clone, Debug)]
pub struct ValidatorInput {
    pub name: String,
    pub version: Option<String>,
}

/// One enum member and its description.
#[derive(Clone, Debug)]
pub struct MemberInput {
    pub name: String,
    pub description: Option<String>,
}

/// One field of an object type.
#[derive(Clone, Debug)]
pub struct FieldInput {
    pub name: String,
    pub ty: String,
    pub description: Option<String>,
    pub documented_default: Option<Value>,
    /// The field's declared default: literal or computed, as the bound
    /// fallback value spells it.
    pub default: Option<DefaultInput>,
    pub sensitivity: Option<String>,
    pub merge: Option<String>,
}

/// A declared default, as a reader holds it: a literal value, or a
/// computed descriptor with its dependencies.
#[derive(Clone, Debug)]
pub struct DefaultInput {
    pub computed: bool,
    pub value: Option<Value>,
    pub dependencies: Vec<String>,
}

impl DefaultInput {
    /// The default a bound fallback value spells: a scalar is a literal,
    /// a single-key map names the union member a case dispatch selected.
    /// `None` spells no recognizable default; the caller rejects it
    /// rather than dropping it silently.
    pub fn from_value(value: &Value) -> Option<Self> {
        let spec = crate::types::DefaultSpec::from_bound(value)?;
        Some(match spec {
            crate::types::DefaultSpec::Literal(value) => Self {
                computed: false,
                value: Some(value),
                dependencies: Vec::new(),
            },
            crate::types::DefaultSpec::Computed { dependencies } => Self {
                computed: true,
                value: None,
                dependencies,
            },
        })
    }
}

/// A `union` variant: its discriminator, type, and value prefix.
#[derive(Clone, Debug)]
pub struct VariantInput {
    pub discriminator: String,
    pub ty: String,
    pub prefix: Option<String>,
}

/// An object constraint, as declared.
#[derive(Clone, Debug)]
pub enum ConstraintInput {
    AllOrNone {
        fields: Vec<String>,
    },
    Requires {
        field: String,
        required: String,
    },
    Bound {
        field: String,
        min: Option<u64>,
        max: Option<u64>,
    },
    AtMostOne {
        fields: Vec<String>,
    },
    ExactlyOneField {
        fields: Vec<String>,
    },
}

/// A node declaration: role, type, placement, profile, field bindings,
/// child rules, effect groups, exactly-one groups, descendant rules, the
/// ordered sequence, and the identity, dependent-value, and case-dispatch
/// blocks it declares.
#[derive(Clone, Debug, Default)]
pub struct NodeInput {
    pub name: String,
    pub role: Option<String>,
    pub ty: Option<String>,
    pub parent: Option<String>,
    pub parents: Option<Vec<String>>,
    /// The node group the `parents` child names with `group=`: the node
    /// is placed under every member as well.
    pub parent_group: Option<String>,
    /// `root=`: with `#true`, a node placed under parents is admitted at
    /// the root as well.
    pub root: Option<bool>,
    /// The profile the declaration belongs to (`profile=NAME`).
    pub profile: Option<String>,
    pub fields: Vec<FieldBindingInput>,
    pub children: Vec<ChildRuleInput>,
    /// The `children-group` rules, keyed by node group name; each
    /// expands to one children rule per member.
    pub children_groups: Vec<ChildRuleInput>,
    pub effects: Vec<EffectGroupInput>,
    pub groups: Vec<Vec<String>>,
    pub descendants: Vec<DescendantRuleInput>,
    pub sequence: Option<SequenceInput>,
    pub value: Option<DependentInput>,
    pub cases: Vec<CaseInput>,
    pub otherwise: Option<String>,
    pub identity: Option<IdentityInput>,
    /// The declaration's `description=` documentation.
    pub description: Option<String>,
    /// The annotation facts the declaration carries, in written order.
    pub annotations: Vec<NodeAnnotationInput>,
}

/// One field binding and the sources that populate it.
#[derive(Clone, Debug, Default)]
pub struct FieldBindingInput {
    pub name: String,
    pub ty: Option<String>,
    pub sources: Vec<SourceInput>,
    pub collision: Option<String>,
    pub duplicate_keys: Option<String>,
}

/// Where a field's value comes from; shorthand spellings and `from`
/// blocks arrive as the same sources.
#[derive(Clone, Debug)]
pub enum SourceInput {
    Argument(u64),
    Args,
    Property(String),
    Child {
        name: String,
        sources: Vec<SourceInput>,
    },
    Children {
        name: String,
        sources: Vec<SourceInput>,
    },
    Entries {
        node: String,
        key: String,
        value: String,
    },
    /// Declaration-group expansion: the named keyed member of the named
    /// declaration splices into the collection being bound.
    Expand {
        target: String,
        member: String,
    },
    Context {
        slot: String,
        policy: Option<String>,
    },
}

/// A `children NAME` rule's occurrence bounds.
#[derive(Clone, Debug, Default)]
pub struct ChildRuleInput {
    pub name: String,
    pub min: Option<u64>,
    pub max: Option<u64>,
}

/// An effect group: its target and effects.
#[derive(Clone, Debug, Default)]
pub struct EffectGroupInput {
    pub target: String,
    pub effects: Vec<EffectInput>,
}

/// One context effect, as declared.
#[derive(Clone, Debug)]
pub enum EffectInput {
    Set {
        slot: String,
        field: Option<String>,
        value: Option<Value>,
        /// The layer a set on a layered slot names.
        layer: Option<String>,
    },
    Remove {
        slot: String,
    },
    Clear {
        namespace: String,
    },
    Handler {
        name: String,
    },
}

/// A typed context slot.
#[derive(Clone, Debug, Default)]
pub struct SlotInput {
    pub name: String,
    pub ty: String,
    pub namespace: Option<String>,
    pub description: Option<String>,
    /// The accumulation operation the slot declares, when it declares
    /// one: `append` or `merge`.
    pub accumulate: Option<String>,
    /// The accumulation's duplicate treatment, when it declares one:
    /// `reject` or `last-wins`.
    pub accumulate_duplicates: Option<String>,
    /// The slot's duplicate policy, when it declares one: `reject`,
    /// `last-wins`, or `layered`. Absent is the fail-closed `reject`.
    pub duplicates: Option<String>,
    /// The layer order of a layered slot, highest precedence first.
    pub layers: Option<Vec<String>>,
}

/// The document rule: first-node choices and following nodes.
#[derive(Clone, Debug, Default)]
pub struct DocumentRuleInput {
    pub first: Option<Vec<String>>,
    pub following: Vec<String>,
}

/// A source import as written.
#[derive(Clone, Debug, Default)]
pub struct ImportInput {
    pub path: String,
    pub project: Vec<String>,
    pub repeated: String,
    /// The alias an `as=NAME` import carries.
    pub alias: Option<String>,
}

/// A document profile, as declared.
#[derive(Clone, Debug, Default)]
pub struct ProfileInput {
    pub name: String,
    /// The profile this one extends, when it declares one.
    pub extends: Option<String>,
    /// Whether the profile is marked the schema's default; `None` and
    /// `Some(false)` are the same.
    pub default: Option<bool>,
    /// The root types the profile admits, when it declares roots.
    pub roots: Option<Vec<String>>,
    pub grammar: Option<String>,
    /// The match rules of the profile's `match` block, in written order:
    /// each rule's node name (`path-suffix`, `path-prefix`, or
    /// `file-name`) and its text. `None` when it declares no block.
    pub rules: Option<Vec<MatchRuleInput>>,
}

/// One profile match rule, as declared.
#[derive(Clone, Debug, Default)]
pub struct MatchRuleInput {
    pub kind: String,
    pub text: String,
}

/// A discovery grammar, as declared.
#[derive(Clone, Debug)]
pub struct DiscoveryInput {
    pub name: String,
    pub anchors: Vec<AnchorInput>,
    pub star: bool,
    pub doublestar: bool,
    pub braces: bool,
    pub allow_empty: bool,
}

/// One anchor a discovery grammar declares.
#[derive(Clone, Debug)]
pub struct AnchorInput {
    pub name: String,
    pub provider: String,
}

/// A named reference type, as declared.
#[derive(Clone, Debug)]
pub struct ReferenceInput {
    pub name: String,
    pub target: String,
    pub codec: Option<String>,
    pub lookup: Option<String>,
    pub qualification: String,
    pub qualifiers: Vec<QualifierInput>,
}

/// One qualifier a reference declares.
#[derive(Clone, Debug)]
pub struct QualifierInput {
    pub name: String,
    pub ty: String,
    pub required: bool,
}

/// A schema-level annotation declaration, as declared.
#[derive(Clone, Debug, Default)]
pub struct AnnotationInput {
    pub name: String,
    /// The declared type expression, as written.
    pub ty: String,
    /// Whether every node declaration must carry the annotation.
    pub required: bool,
}

/// A node group declaration, as declared: its name and member node
/// names, in written order.
#[derive(Clone, Debug, Default)]
pub struct GroupInput {
    pub name: String,
    pub nodes: Vec<String>,
}

/// One annotation fact a node declaration carries, as written.
#[derive(Clone, Debug)]
pub struct NodeAnnotationInput {
    pub name: String,
    pub value: Value,
}

/// One descendant rule of a node declaration, as declared.
#[derive(Clone, Debug)]
pub struct DescendantRuleInput {
    pub name: String,
    pub min: Option<u64>,
    pub max: Option<u64>,
}

/// An ordered sequence a node declaration's children must match, as
/// declared.
#[derive(Clone, Debug)]
pub struct SequenceInput {
    pub steps: Vec<StepInput>,
}

/// One step of an ordered sequence: a node name or an alternatives
/// group, with occurrence bounds. Absent bounds mean exactly once.
#[derive(Clone, Debug)]
pub struct StepInput {
    pub name: Option<String>,
    pub alternatives: Vec<String>,
    pub min: Option<u64>,
    pub max: Option<u64>,
}

/// The identity block of a node declaration, as declared.
#[derive(Clone, Debug)]
pub struct IdentityInput {
    pub domain: String,
    /// The composition form; `qualified-name` is the only defined one.
    pub form: String,
    pub fields: Vec<String>,
}

/// The declared contribution-resolution policy, as declared.
#[derive(Clone, Debug, Default)]
pub struct ResolutionInput {
    pub resolver: String,
    /// Layer names in precedence order, highest first.
    pub layers: Vec<String>,
    pub selection: String,
    pub duplicates: String,
    pub missing: String,
}

/// A secret-provider descriptor, as declared: never executed.
#[derive(Clone, Debug, Default)]
pub struct ProviderInput {
    pub id: String,
    pub ttl: Option<u64>,
    pub command: Vec<String>,
}

/// The declared source-context block, as declared.
#[derive(Clone, Debug, Default)]
pub struct SourceContextInput {
    pub mode: String,
    pub fields: Vec<SourceFieldInput>,
}

/// One source-context field, as declared.
#[derive(Clone, Debug, Default)]
pub struct SourceFieldInput {
    pub name: String,
    pub ty: String,
    /// The registered source-metadata providers it derives from.
    pub sources: Vec<String>,
}
