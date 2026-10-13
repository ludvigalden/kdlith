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
    pub deferrals: Vec<Deferral>,
    pub document_rule: Option<DocumentRule>,
    /// The checks this definition passed.
    pub scope: CheckScope,
}

impl Definition {
    /// Whether this dialect marks a mechanism deferred.
    pub fn defers(&self, mechanism: &str) -> bool {
        self.deferrals.iter().any(|d| d.mechanism == mechanism)
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
    pub site: Site,
}

/// A source import as written; the loader resolves it before compiling.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportDecl {
    pub path: String,
    pub project: Vec<Sym>,
    pub repeated: Repeated,
    pub site: Site,
}

/// A mechanism the dialect deliberately does not define.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Deferral {
    pub mechanism: String,
    pub note: String,
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
