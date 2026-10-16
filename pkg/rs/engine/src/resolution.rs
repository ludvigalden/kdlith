//! Contribution resolution: the preserved records, the declared policy,
//! and the routing surface that hands them to the registered resolver.
//! The engine never resolves contributions itself.

use std::sync::Arc;

use kdlith_model::ResolvedContribution;
use kdlith_runtime::{ContributionPolicy, ContributionRecord, ContributionResolver};

use crate::definition::{Definition, ResolutionPolicyDecl};

/// A registered contribution resolver, shared cheaply across readings.
pub type SharedContributionResolver = Arc<dyn ContributionResolver>;

/// Why a resolution run could not proceed. Every failure is loud: an
/// uninstalled resolver is rejected at compile time already, and a
/// resolver that refuses its records reports its own reason.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionError {
    /// The schema declares no resolution policy, so there is nothing to
    /// route the records to.
    NoPolicy,
    /// The registered resolver is not the one the policy names.
    ResolverMismatch { declared: String, installed: String },
    /// The resolver failed; its reason, verbatim.
    Failed(String),
}

impl ResolutionError {
    /// The registered diagnostic code the failure reports under.
    pub fn code(&self) -> &'static str {
        match self {
            ResolutionError::NoPolicy | ResolutionError::ResolverMismatch { .. } => "KDLITH-DLC006",
            ResolutionError::Failed(_) => "KDLITH-DLC010",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ResolutionError::NoPolicy => "the schema declares no resolution policy".to_owned(),
            ResolutionError::ResolverMismatch {
                declared,
                installed,
            } => format!("the policy names resolver {declared} but {installed} is registered"),
            ResolutionError::Failed(reason) => reason.clone(),
        }
    }
}

/// Route `records` to `resolver` under the definition's declared
/// policy. Fails closed when the schema declares no policy or the
/// registered resolver is not the one the policy names.
pub fn resolve(
    definition: &Definition,
    resolver: &dyn ContributionResolver,
    records: &[ContributionRecord],
) -> Result<Vec<ResolvedContribution>, ResolutionError> {
    let policy = definition
        .resolution
        .as_ref()
        .ok_or(ResolutionError::NoPolicy)?;
    let declared = definition.names.resolve(policy.resolver).to_owned();
    let installed = resolver.name().to_owned();
    if declared != installed {
        return Err(ResolutionError::ResolverMismatch {
            declared,
            installed,
        });
    }
    resolver
        .resolve(&policy_of(policy), records)
        .map_err(ResolutionError::Failed)
}

/// The descriptors a resolver receives for a compiled policy.
fn policy_of(policy: &ResolutionPolicyDecl) -> ContributionPolicy {
    ContributionPolicy::new(
        policy.layers.clone(),
        policy.selection,
        policy.duplicates,
        policy.missing,
    )
}
