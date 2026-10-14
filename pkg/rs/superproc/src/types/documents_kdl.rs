//! Logical types contributed by `documents.kdl`: definition roots,
//! structural document rules, source imports, and the documents
//! surface — profiles, sequences, discovery, the symbol catalog, and
//! the descriptor types.

use kdlith_engine::{
    OrderedMap, Shaped, TypeShape, ValueExpr, enum_type, object_type, string_type, union_type,
};

use super::{FieldName, NodeName, SlotName, TypeExpression, TypeName};

string_type! {
    pub struct DialectName;
}

string_type! {
    pub struct SchemaName;
}

string_type! {
    pub struct ProfileName;
}

string_type! {
    pub struct DiscoveryName;
}

string_type! {
    pub struct AnchorName;
}

string_type! {
    pub struct ProviderName;
}

string_type! {
    pub struct DomainName;
}

string_type! {
    pub struct ResolverName;
}

string_type! {
    pub struct AliasName;
}

string_type! {
    pub struct SecretId;
}

object_type! {
    pub struct DialectDef {
        "name" => name: DialectName,
        "version" => version: String,
    }
}

object_type! {
    pub struct SchemaDef {
        "name" => name: SchemaName,
        "version" => version: Option<String>,
        "grammar" => grammar: Option<DiscoveryName>,
    }
}

object_type! {
    pub struct DocumentRule {
        "first" => first: Option<FirstRule>,
        "following" => following: Vec<NodeName>,
    }
}

object_type! {
    pub struct FirstRule {
        "exactly-one" => exactly_one: Vec<NodeName>,
    }
}

object_type! {
    pub struct ExactlyOneGroup {
        "nodes" => nodes: Vec<NodeName>,
    }
}

enum_type! {
    pub enum RepeatedImport {
        "reject" => Reject,
        "coalesce" => Coalesce,
        "instantiate" => Instantiate,
    }
}

object_type! {
    pub struct ImportRequest {
        "path" => path: String,
        "project" => project: Option<Vec<SlotName>>,
        "repeated" => repeated: RepeatedImport,
        // The imported declarations are reachable through the alias
        // namespace and not by their unqualified names.
        "alias" => alias: Option<AliasName>,
    }
}

object_type! {
    /// A named selection of root types; the processing invocation names
    /// the profile explicitly, never inferred from document content.
    pub struct ProfileDef {
        "name" => name: ProfileName,
        "roots" => roots: Option<RootsRule>,
        "grammar" => grammar: Option<DiscoveryName>,
    }
}

object_type! {
    pub struct RootsRule {
        "allow" => allow: Vec<TypeName>,
    }
}

object_type! {
    /// An occurrence bound over the declaring node's whole subtree,
    /// checked when the declaring scope completes.
    pub struct DescendantRule {
        "name" => name: NodeName,
        "min-occurs" => min_occurs: Option<u64>,
        "max-occurs" => max_occurs: Option<u64>,
    }
}

object_type! {
    /// An ordered sequence the declaring node's immediate children must
    /// match; repetition stays with the step's occurrence bounds.
    pub struct SequenceRule {
        "steps" => steps: Vec<SequenceStep>,
    }
}

object_type! {
    pub struct SequenceStep {
        "name" => name: Option<NodeName>,
        "alternatives" => alternatives: Option<AlternativesRule>,
        "min-occurs" => min_occurs: Option<u64>,
        "max-occurs" => max_occurs: Option<u64>,
    }
}

object_type! {
    pub struct AlternativesRule {
        "nodes" => nodes: Vec<NodeName>,
    }
}

object_type! {
    /// The restricted path-expansion grammar import expressions are
    /// evaluated under; anchors are host-supplied typed paths.
    pub struct DiscoveryDef {
        "name" => name: DiscoveryName,
        "anchors" => anchors: OrderedMap<AnchorName, AnchorDef>,
        "patterns" => patterns: Option<PatternsRule>,
        "order" => order: DiscoveryOrder,
        "allow-empty" => allow_empty: Option<bool>,
    }
}

object_type! {
    /// The anchor's name is the key of the enclosing ordered map.
    pub struct AnchorDef {
        "provider" => provider: ProviderName,
    }
}

object_type! {
    /// The pattern syntax a discovery grammar admits; absent means not
    /// admitted.
    pub struct PatternsRule {
        "star" => star: Option<bool>,
        "doublestar" => doublestar: Option<bool>,
        "braces" => braces: Option<bool>,
    }
}

enum_type! {
    /// The only defined ordering policy: matched sources are ordered by
    /// their portable path bytes.
    pub enum DiscoveryOrder {
        "portable-path-bytes" => PortablePathBytes,
    }
}

object_type! {
    /// The identity block of a declaration: the domain its qualified
    /// symbol belongs to and how the symbol composes from bound fields.
    pub struct IdentityBlock {
        "domain" => domain: DomainDecl,
        "compose" => compose: ComposeRule,
    }
}

object_type! {
    pub struct DomainDecl {
        "name" => name: DomainName,
    }
}

object_type! {
    pub struct ComposeRule {
        "form" => form: ComposeForm,
        "fields" => fields: Vec<FieldName>,
    }
}

enum_type! {
    pub enum ComposeForm {
        "qualified-name" => QualifiedName,
    }
}

object_type! {
    /// A named reference type: its target, codec, lookup domain,
    /// qualification policy, and declared qualifiers.
    pub struct ReferenceDef {
        "name" => name: TypeName,
        "target" => target: TypeName,
        "codec" => codec: Option<TypeName>,
        "lookup" => lookup: Option<DomainName>,
        "qualification" => qualification: Qualification,
        "qualifiers" => qualifiers: Vec<QualifierRule>,
    }
}

enum_type! {
    pub enum Qualification {
        "explicit" => Explicit,
        "explicit-or-context" => ExplicitOrContext,
        "imported-or-context" => ImportedOrContext,
    }
}

object_type! {
    pub struct QualifierRule {
        "name" => name: FieldName,
        "type" => ty: TypeExpression,
        // A declared qualifier is required unless it says otherwise.
        "required" => required: Option<bool>,
    }
}

object_type! {
    /// The contribution-resolution policy: the registered resolver and
    /// the descriptors of how it applies contributions across layers.
    pub struct ResolutionPolicyDef {
        "resolver" => resolver: ResolverName,
        "layers" => layers: Option<Vec<String>>,
        "selection" => selection: ResolutionSelection,
        "duplicates" => duplicates: DuplicateTreatment,
        "missing" => missing: MissingRequirement,
    }
}

enum_type! {
    pub enum ResolutionSelection {
        "replace" => Replace,
        "union" => Union,
    }
}

enum_type! {
    pub enum DuplicateTreatment {
        "reject" => Reject,
        "last-wins" => LastWins,
    }
}

enum_type! {
    pub enum MissingRequirement {
        "reject" => Reject,
        "allow" => Allow,
    }
}

union_type! {
    /// A field's fallback: a literal value that supplies an absent
    /// field, or a computed descriptor the processor never computes.
    pub enum DefaultSpec {
        "literal" => Literal(LiteralDefault),
        "computed" => Computed(ComputedDefault),
    }
}

object_type! {
    pub struct LiteralDefault {
        "value" => value: ValueExpr,
    }
}

object_type! {
    /// A computed default: computation belongs to the owning domain
    /// process, never to the processor.
    pub struct ComputedDefault {
        "dependencies" => dependencies: Vec<String>,
    }
}

object_type! {
    /// A secret provider: descriptor data only, never executed and
    /// never reinterpreted as a shell command string.
    pub struct SecretProvider {
        "id" => id: SecretId,
        "ttl" => ttl: Option<u64>,
        "command" => command: Vec<String>,
    }
}

/// The shapes of this document's types, in declaration order.
pub(crate) const TYPES: &[TypeShape] = &[
    DialectName::SHAPE,
    SchemaName::SHAPE,
    ProfileName::SHAPE,
    DiscoveryName::SHAPE,
    AnchorName::SHAPE,
    ProviderName::SHAPE,
    DomainName::SHAPE,
    ResolverName::SHAPE,
    AliasName::SHAPE,
    SecretId::SHAPE,
    DialectDef::SHAPE,
    SchemaDef::SHAPE,
    DocumentRule::SHAPE,
    FirstRule::SHAPE,
    ExactlyOneGroup::SHAPE,
    RepeatedImport::SHAPE,
    ImportRequest::SHAPE,
    ProfileDef::SHAPE,
    RootsRule::SHAPE,
    DescendantRule::SHAPE,
    SequenceRule::SHAPE,
    SequenceStep::SHAPE,
    AlternativesRule::SHAPE,
    DiscoveryDef::SHAPE,
    AnchorDef::SHAPE,
    PatternsRule::SHAPE,
    DiscoveryOrder::SHAPE,
    IdentityBlock::SHAPE,
    DomainDecl::SHAPE,
    ComposeRule::SHAPE,
    ComposeForm::SHAPE,
    ReferenceDef::SHAPE,
    Qualification::SHAPE,
    QualifierRule::SHAPE,
    ResolutionPolicyDef::SHAPE,
    ResolutionSelection::SHAPE,
    DuplicateTreatment::SHAPE,
    MissingRequirement::SHAPE,
    DefaultSpec::SHAPE,
    LiteralDefault::SHAPE,
    ComputedDefault::SHAPE,
    SecretProvider::SHAPE,
];
