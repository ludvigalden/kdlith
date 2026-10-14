//! The SymbolCatalog: identity domains, declaration descriptors with
//! their identity composition, and the named reference policies.

use crate::bindings::{BindingCatalog, IdentityDecl, NodeDeclId, Role, Source};
use crate::definition::ReferenceDecl;
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
    /// The identity domain: the `identity` block's declared domain, the
    /// node's `type=`, or its name when untyped. A `ref<T>` targets the
    /// declarations whose domain is `T`.
    pub domain: Sym,
    /// How the declaration's qualified symbol composes, when it declares
    /// an identity block; `None` falls back to the field bound from
    /// argument 0, or to one-of-a-kind in its scope.
    pub identity: Option<IdentityDecl>,
    /// The fields that carry the identity: the identity block's
    /// composition order, or argument 0's field.
    pub identity_fields: Vec<Sym>,
    pub policy: ReferencePolicy,
}

/// A node whose bound result is preserved for later resolution; resolving
/// contributions is the registered resolver's work, never the engine's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContributionDescriptor {
    pub node: NodeDeclId,
    pub target: Option<Sym>,
}

/// The declarations, contributions, and named reference policies of one
/// definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SymbolCatalog {
    declarations: Vec<DeclarationDescriptor>,
    contributions: Vec<ContributionDescriptor>,
    references: Vec<ReferenceDecl>,
}

impl SymbolCatalog {
    pub(crate) fn build(bindings: &BindingCatalog, references: &[ReferenceDecl]) -> Self {
        let mut catalog = Self::default();
        for (id, node) in bindings.iter() {
            match node.role {
                Some(Role::Declaration) => {
                    let argument0 = node
                        .fields
                        .iter()
                        .find(|field| field.sources.contains(&Source::Argument(0)))
                        .map(|field| field.name);
                    let (domain, identity, identity_fields) = match &node.identity {
                        Some(identity) => (
                            identity.domain,
                            Some(identity.clone()),
                            identity.fields.clone(),
                        ),
                        None => (
                            node.ty.unwrap_or(node.name),
                            None,
                            argument0.into_iter().collect(),
                        ),
                    };
                    catalog.declarations.push(DeclarationDescriptor {
                        node: id,
                        domain,
                        identity,
                        identity_fields,
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
        catalog.references = references.to_vec();
        catalog
    }

    pub fn declarations(&self) -> &[DeclarationDescriptor] {
        &self.declarations
    }

    pub fn contributions(&self) -> &[ContributionDescriptor] {
        &self.contributions
    }

    /// The named reference policies, in declaration order.
    pub fn references(&self) -> &[ReferenceDecl] {
        &self.references
    }

    /// The named reference policy `name` declares, if any.
    pub fn reference(&self, name: Sym) -> Option<&ReferenceDecl> {
        self.references.iter().find(|r| r.name == name)
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

    /// The declarations an alias namespace gates: under an aliased
    /// import, the imported declarations are reachable through the alias
    /// and not by their unqualified names.
    pub fn declarations_in(&self, domain: Sym) -> impl Iterator<Item = &DeclarationDescriptor> {
        self.declarations.iter().filter(move |d| d.domain == domain)
    }
}

impl ReferenceDecl {
    /// The qualification policy as its declaration spells it.
    pub fn qualification_name(&self) -> &'static str {
        self.qualification.name()
    }
}
