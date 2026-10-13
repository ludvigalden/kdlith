//! The minimal SymbolCatalog: declaring nodes, their identity domain and
//! field, and by-name `ref<T>` lookup. Richer domains and lookup policies
//! are the deferred `symbol-catalogs` mechanism.

use crate::bindings::{BindingCatalog, NodeDeclId, Role, Source};
use crate::names::Sym;

/// How a reference finds a declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferencePolicy {
    /// By the declared name, within the declaring type's domain.
    ByName,
}

/// A node declaration that registers an identity.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeclarationDescriptor {
    pub node: NodeDeclId,
    /// The identity domain: the node's `type=`, or its name when untyped.
    /// A `ref<T>` targets the declarations whose domain is `T`.
    pub domain: Sym,
    /// The field bound from argument 0 that carries the identity; `None`
    /// for a declaration that is one of a kind in its scope.
    pub identity: Option<Sym>,
    pub policy: ReferencePolicy,
}

/// A node whose bound result is preserved for later resolution; resolving
/// contributions is the deferred `contribution-resolution` mechanism.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContributionDescriptor {
    pub node: NodeDeclId,
    pub target: Option<Sym>,
}

/// The declarations and contributions of one definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SymbolCatalog {
    declarations: Vec<DeclarationDescriptor>,
    contributions: Vec<ContributionDescriptor>,
}

impl SymbolCatalog {
    pub(crate) fn build(bindings: &BindingCatalog) -> Self {
        let mut catalog = Self::default();
        for (id, node) in bindings.iter() {
            match node.role {
                Some(Role::Declaration) => {
                    let identity = node
                        .fields
                        .iter()
                        .find(|field| field.sources.contains(&Source::Argument(0)))
                        .map(|field| field.name);
                    catalog.declarations.push(DeclarationDescriptor {
                        node: id,
                        domain: node.ty.unwrap_or(node.name),
                        identity,
                        policy: ReferencePolicy::ByName,
                    });
                }
                Some(Role::Contribution) => {
                    catalog.contributions.push(ContributionDescriptor {
                        node: id,
                        target: node.ty,
                    });
                }
                _ => {}
            }
        }
        catalog
    }

    pub fn declarations(&self) -> &[DeclarationDescriptor] {
        &self.declarations
    }

    pub fn contributions(&self) -> &[ContributionDescriptor] {
        &self.contributions
    }

    /// Whether some declaration registers identities in `domain`, so a
    /// `ref<domain>` can resolve.
    pub fn declares(&self, domain: Sym) -> bool {
        self.declarations.iter().any(|d| d.domain == domain)
    }

    /// The descriptor of a declaring node.
    pub fn declaration(&self, node: NodeDeclId) -> Option<&DeclarationDescriptor> {
        self.declarations.iter().find(|d| d.node == node)
    }
}
