//! A compiled dialect or schema definition: the catalogs plus the
//! declarations later compile stages consume.

use kdlith_runtime::Repeated;

use crate::bindings::BindingCatalog;
use crate::diag::Site;
use crate::names::{Names, Sym};
use crate::plan::ProcessingPlan;
use crate::symbols::SymbolCatalog;
use crate::types::{TypeCatalog, TypeExpr};

/// Which authoring level a definition belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DefinitionKind {
    Dialect,
    Schema,
}

impl DefinitionKind {
    pub fn root_name(self) -> &'static str {
        match self {
            DefinitionKind::Dialect => "dialect",
            DefinitionKind::Schema => "schema",
        }
    }

    pub(crate) fn origin(self) -> &'static str {
        match self {
            DefinitionKind::Dialect => "dialect-definition",
            DefinitionKind::Schema => "schema-definition",
        }
    }
}

/// How much of a definition's checking runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CheckScope {
    /// Syntax, constructs, and bindings of the named documents alone:
    /// the checks one document supports while it streams. Imports are
    /// recorded but not followed, and nothing is linked.
    Local,
    /// Additionally resolve imports, link every reference across the
    /// contributing documents, and check self-description.
    #[default]
    Complete,
}

impl CheckScope {
    pub fn name(self) -> &'static str {
        match self {
            CheckScope::Local => "local",
            CheckScope::Complete => "complete",
        }
    }
}

/// One definition compiled from its contributing documents. Records are
/// processed against these tables; the definition's KDL is never read
/// again.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Definition {
    pub kind: DefinitionKind,
    pub name: String,
    pub version: Option<String>,
    /// The contributing documents' paths, indexed by [`Site::document`].
    pub documents: Vec<String>,
    pub names: Names,
    pub types: TypeCatalog,
    pub bindings: BindingCatalog,
    pub symbols: SymbolCatalog,
    pub plan: ProcessingPlan,
    pub slots: Vec<SlotDecl>,
    pub imports: Vec<ImportDecl>,
    pub profiles: Vec<ProfileDecl>,
    pub discoveries: Vec<DiscoveryDecl>,
    /// The discovery grammar the definition's root binds by property,
    /// interned; import expressions evaluate under it when no profiles
    /// are declared.
    pub root_grammar: Option<Sym>,
    pub references: Vec<ReferenceDecl>,
    /// The declared contribution-resolution policy, when the schema
    /// declares one. The engine compiles and holds it; the registered
    /// resolver applies it.
    pub resolution: Option<ResolutionPolicyDecl>,
    /// The secret-provider descriptors the schema declares. Descriptor
    /// data only: nothing in the engine or its processors ever executes
    /// a provider command.
    pub providers: Vec<ProviderDecl>,
    /// The declared source-context block, when the schema declares one.
    pub source_context: Option<SourceContextDecl>,
    pub document_rule: Option<DocumentRule>,
    /// The checks this definition passed.
    pub scope: CheckScope,
}

impl Definition {
    /// The discovery grammar import expressions evaluate under for an
    /// invocation naming `profile`: the profile's bound grammar, or the
    /// root's when no profiles are declared. `None` evaluates literal
    /// paths only; a profile-binding definition with no profile named
    /// binds none, exactly as dispatch fails closed without one.
    pub fn grammar_for(&self, profile: Option<Sym>) -> Option<&DiscoveryDecl> {
        let bound = match profile {
            Some(named) => self.profiles.iter().find(|p| p.name == named)?.grammar?,
            None if self.profiles.is_empty() => self.root_grammar?,
            None => return None,
        };
        self.discoveries.iter().find(|decl| decl.name == bound)
    }
}

/// A typed context slot.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SlotDecl {
    pub name: Sym,
    pub ty: TypeExpr,
    pub namespace: Option<Sym>,
    pub description: Option<String>,
    /// How an accumulating set combines with the value the slot already
    /// holds; `None` leaves sets plain.
    pub accumulate: Option<AccumulateOp>,
    /// How a `merge` accumulation treats a key the held map already
    /// carries; `Reject` for every slot that does not merge.
    pub merge_duplicates: MergeDuplicates,
    /// How a plain set reaching an occupied slot is treated.
    pub duplicates: SlotDuplicates,
    /// The layer order of a layered slot, highest precedence first;
    /// empty for every other slot.
    pub layers: Vec<Sym>,
    pub site: Site,
}

/// How a set reaching a slot that already holds a value is treated when
/// the slot does not accumulate. The fail-closed default rejects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SlotDuplicates {
    /// A second value is rejected (`KDLITH-CTX006`).
    #[default]
    Reject,
    /// The later value replaces the held one.
    LastWins,
    /// A higher layer's value replaces a lower layer's; a lower layer's
    /// later value leaves the held value in place; a second value within
    /// one layer, or meeting a held value without a layer, is rejected.
    Layered,
}

impl SlotDuplicates {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "reject" => SlotDuplicates::Reject,
            "last-wins" => SlotDuplicates::LastWins,
            "layered" => SlotDuplicates::Layered,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            SlotDuplicates::Reject => "reject",
            SlotDuplicates::LastWins => "last-wins",
            SlotDuplicates::Layered => "layered",
        }
    }
}

/// How a `merge` accumulation treats a key contributed twice. The
/// fail-closed default rejects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MergeDuplicates {
    /// A key contributed twice is a duplicate-key rejection
    /// (`KDLITH-BND006`), never a silent overwrite.
    #[default]
    Reject,
    /// The later entry replaces the held entry's value, keeping the
    /// key's first position.
    LastWins,
}

impl MergeDuplicates {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "reject" => MergeDuplicates::Reject,
            "last-wins" => MergeDuplicates::LastWins,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MergeDuplicates::Reject => "reject",
            MergeDuplicates::LastWins => "last-wins",
        }
    }
}

/// How an accumulating effect combines with the slot's incoming value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AccumulateOp {
    /// Concatenate lists in effect order; for list-typed slots.
    Append,
    /// Combine maps key-wise under the slot's declared merge duplicate
    /// treatment.
    Merge,
}

impl AccumulateOp {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "append" => AccumulateOp::Append,
            "merge" => AccumulateOp::Merge,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            AccumulateOp::Append => "append",
            AccumulateOp::Merge => "merge",
        }
    }
}

/// A source import as written; the loader resolves it before compiling.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportDecl {
    pub path: String,
    pub project: Vec<Sym>,
    pub repeated: Repeated,
    /// The alias an `as=NAME` import carries, if any: the imported
    /// declarations are reachable through the alias namespace and not by
    /// their unqualified names.
    pub alias: Option<Sym>,
    pub site: Site,
}

/// A document profile: a named selection of root types the invocation
/// names explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileDecl {
    pub name: Sym,
    /// The root types the profile admits; `None` admits every declared
    /// root.
    pub roots: Option<Vec<Sym>>,
    /// The discovery grammar the profile binds, by name.
    pub grammar: Option<Sym>,
    pub site: Site,
}

/// The pattern syntax a discovery grammar admits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Patterns {
    pub star: bool,
    pub doublestar: bool,
    pub braces: bool,
}

/// A discovery grammar: the restricted path-expansion grammar import
/// expressions are evaluated under.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiscoveryDecl {
    pub name: Sym,
    pub anchors: Vec<AnchorDecl>,
    pub patterns: Patterns,
    /// `true` when an expression matching zero sources is allowed to
    /// resolve to nothing; otherwise it does not resolve.
    pub allow_empty: bool,
    pub site: Site,
}

/// One host-supplied typed-path anchor a grammar names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnchorDecl {
    pub name: Sym,
    pub provider: Sym,
    pub site: Site,
}

/// A named reference type: how a reference of this name parses, looks
/// up, and qualifies its target.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceDecl {
    pub name: Sym,
    /// The type of declaration the reference targets; its identity
    /// domain.
    pub target: Sym,
    /// The registered syntax codec that parses the reference's spelling,
    /// and the reference namespace a decoded identity belongs to when
    /// the codec declares one.
    pub codec: Option<Sym>,
    pub namespace: Option<Sym>,
    /// The lookup domain, when it differs from the target's.
    pub lookup: Option<Sym>,
    pub qualification: Qualification,
    pub qualifiers: Vec<QualifierDecl>,
    pub site: Site,
}

/// How a reference qualifies its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Qualification {
    /// The reference spells its qualification completely.
    Explicit,
    /// The reference may take missing qualifiers from context.
    ExplicitOrContext,
    /// The reference may resolve through an import alias or context.
    ImportedOrContext,
}

impl Qualification {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "explicit" => Qualification::Explicit,
            "explicit-or-context" => Qualification::ExplicitOrContext,
            "imported-or-context" => Qualification::ImportedOrContext,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Qualification::Explicit => "explicit",
            Qualification::ExplicitOrContext => "explicit-or-context",
            Qualification::ImportedOrContext => "imported-or-context",
        }
    }
}

/// One declared qualifier of a reference.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct QualifierDecl {
    pub name: Sym,
    pub ty: TypeExpr,
    /// A declared qualifier is required unless it says otherwise.
    pub required: bool,
    pub site: Site,
}

/// Rules over the root nodes of the documents a definition reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentRule {
    pub first: Option<Vec<Sym>>,
    pub following: Vec<Sym>,
    pub site: Site,
}

/// The declared contribution-resolution policy: the registered resolver
/// that resolves the schema's contributions across layers, and the
/// descriptors of how it applies. The engine compiles and holds the
/// policy; resolution itself belongs to the resolver the processor
/// registers, never to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolutionPolicyDecl {
    /// The registered resolver the policy names; a resolver the processor
    /// does not install fails closed at compile time.
    pub resolver: Sym,
    /// The layer names in precedence order, highest first. Absent means
    /// the resolver applies its own layer ordering.
    pub layers: Vec<String>,
    /// How multiple contributions to one target combine.
    pub selection: ResolutionSelection,
    /// How a layer contributing the same target twice is treated.
    pub duplicates: DuplicateTreatment,
    /// Whether a target no contribution reaches is an error.
    pub missing: MissingRequirement,
    pub site: Site,
}

/// How contributions to one target combine under the resolution policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResolutionSelection {
    /// The highest-precedence contribution replaces the rest.
    #[default]
    Replace,
    /// Contributions union their values, per the resolver's contract.
    Union,
}

impl ResolutionSelection {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "replace" => ResolutionSelection::Replace,
            "union" => ResolutionSelection::Union,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            ResolutionSelection::Replace => "replace",
            ResolutionSelection::Union => "union",
        }
    }
}

/// How a layer contributing the same target twice is treated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DuplicateTreatment {
    /// A twice-contributed target is a duplicate rejection.
    #[default]
    Reject,
    /// The later contribution wins, per the resolver's contract.
    LastWins,
}

impl DuplicateTreatment {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "reject" => DuplicateTreatment::Reject,
            "last-wins" => DuplicateTreatment::LastWins,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            DuplicateTreatment::Reject => "reject",
            DuplicateTreatment::LastWins => "last-wins",
        }
    }
}

/// Whether a target no contribution reaches is an error.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MissingRequirement {
    /// A target without a contribution is an error.
    #[default]
    Reject,
    /// A target without a contribution resolves to nothing.
    Allow,
}

impl MissingRequirement {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "reject" => MissingRequirement::Reject,
            "allow" => MissingRequirement::Allow,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MissingRequirement::Reject => "reject",
            MissingRequirement::Allow => "allow",
        }
    }
}

/// A secret provider as declared: descriptor data only. Nothing in the
/// engine or its processors ever executes `command` or reinterprets it as
/// a shell command string; the list preserves argument boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderDecl {
    pub id: String,
    pub ttl: Option<u64>,
    pub command: Vec<String>,
    pub site: Site,
}

/// The declared source-context block: fields derived from source
/// metadata, applied under the declared mode before the document's first
/// semantic node with their origin recorded in provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceContextDecl {
    pub mode: SourceContextMode,
    pub fields: Vec<SourceFieldDecl>,
    pub site: Site,
}

/// How derived source-context values relate to values already present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SourceContextMode {
    /// The derived values apply before any control.
    Initial,
    /// The derived values supply slots only where none is set.
    Fallback,
    /// The derived value applies; an explicit value that later reads the
    /// slot under policy fixed must agree with it.
    Fixed,
}

impl SourceContextMode {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "initial" => SourceContextMode::Initial,
            "fallback" => SourceContextMode::Fallback,
            "fixed" => SourceContextMode::Fixed,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            SourceContextMode::Initial => "initial",
            SourceContextMode::Fallback => "fallback",
            SourceContextMode::Fixed => "fixed",
        }
    }
}

/// One source-context field: the slot it feeds, the type its derived
/// value is validated against, and the registered source-metadata
/// providers it derives from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceFieldDecl {
    pub name: Sym,
    pub ty: TypeExpr,
    /// The registered source-metadata providers the value derives from;
    /// `path-stem` is the one this version defines.
    pub sources: Vec<String>,
    pub site: Site,
}
