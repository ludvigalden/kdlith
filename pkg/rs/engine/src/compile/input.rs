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

/// One construct with the span of the node that declared it.
#[derive(Clone, Debug)]
pub struct Located<T> {
    pub item: T,
    pub span: ByteSpan,
}

/// One declaration or request a definition document makes.
#[derive(Clone, Debug)]
pub enum Construct {
    Type(TypeInput),
    Node(NodeInput),
    Slot(SlotInput),
    Rule(DocumentRuleInput),
    Import(ImportInput),
    Deferral(DeferralInput),
}

/// A logical type declaration: what `string`, `enum`, `object`,
/// `union`, and `map` declare.
#[derive(Clone, Debug)]
pub enum TypeInput {
    String {
        name: String,
        format: Option<FormatInput>,
        pattern: Option<String>,
        validator: Option<ValidatorInput>,
        min_length: Option<u64>,
    },
    Enum {
        name: String,
        members: Vec<MemberInput>,
    },
    Object {
        name: String,
        fields: Vec<FieldInput>,
        constraints: Vec<ConstraintInput>,
    },
    Union {
        name: String,
        variants: Vec<VariantInput>,
    },
    Map {
        name: String,
        key: String,
        value: String,
    },
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
    pub fallback: Option<Value>,
}

/// A `union` variant: its discriminator and type.
#[derive(Clone, Debug)]
pub struct VariantInput {
    pub discriminator: String,
    pub ty: String,
}

/// An object constraint, as declared.
#[derive(Clone, Debug)]
pub enum ConstraintInput {
    AllOrNone { fields: Vec<String> },
    Requires { field: String, required: String },
}

/// A node declaration: role, type, placement, field bindings, child
/// rules, effect groups, and exactly-one groups.
#[derive(Clone, Debug, Default)]
pub struct NodeInput {
    pub name: String,
    pub role: Option<String>,
    pub ty: Option<String>,
    pub parent: Option<String>,
    pub parents: Option<Vec<String>>,
    pub fields: Vec<FieldBindingInput>,
    pub children: Vec<ChildRuleInput>,
    pub effects: Vec<EffectGroupInput>,
    pub groups: Vec<Vec<String>>,
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
}

/// A mechanism the dialect defers.
#[derive(Clone, Debug)]
pub struct DeferralInput {
    pub mechanism: String,
    pub note: String,
}
