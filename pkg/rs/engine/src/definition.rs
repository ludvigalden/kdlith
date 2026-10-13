//! A compiled dialect or schema definition: the catalogs plus the
//! declarations later compile stages consume.

use crate::bindings::BindingCatalog;
use crate::diag::Site;
use crate::names::{Names, Sym};
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
    pub slots: Vec<SlotDecl>,
    pub imports: Vec<ImportDecl>,
    pub deferrals: Vec<Deferral>,
    pub document_rule: Option<DocumentRule>,
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

/// How a source imported twice in one session is handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Repeated {
    Reject,
    Coalesce,
    Instantiate,
}

/// A source import as written; resolving it is a later stage.
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
