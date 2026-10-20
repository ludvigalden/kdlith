//! Contribution resolution: the preserved records, the declared policy,
//! and the routing surface that hands them to the registered resolver.
//! The engine never resolves contributions itself.

use std::collections::BTreeMap;
use std::sync::Arc;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::ResolvedContribution;
use kdlith_runtime::{ContributionPolicy, ContributionRecord, ContributionResolver};

use crate::bindings::Role;
use crate::definition::{Definition, ResolutionPolicyDecl};
use crate::diag::record;
use crate::types::TypeKind;

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
        .resolve(&policy_of(definition, policy), records)
        .map_err(ResolutionError::Failed)
}

/// The descriptors a resolver receives for a compiled policy. Records
/// carry no contribution type, so merge policies are keyed by field
/// name across every contribution type.
fn policy_of(definition: &Definition, policy: &ResolutionPolicyDecl) -> ContributionPolicy {
    let mut merges = BTreeMap::new();
    for (_, decl) in definition.bindings.iter() {
        if decl.role != Some(Role::Contribution) {
            continue;
        }
        let Some(ty) = decl.ty.and_then(|name| definition.types.lookup(name)) else {
            continue;
        };
        let Some(TypeKind::Object(object)) = definition.types.get(ty).map(|ty| &ty.kind) else {
            continue;
        };
        for field in &object.fields {
            if let Some(merge) = field.merge {
                merges.insert(definition.names.resolve(field.name).to_owned(), merge);
            }
        }
    }
    ContributionPolicy::new(
        policy.layers.clone(),
        policy.selection,
        policy.duplicates,
        policy.missing,
        merges,
    )
}

/// The diagnostic a resolution run that could not complete reports.
pub(crate) fn failure_record(error: &ResolutionError, origin: &str) -> DiagnosticRecord {
    match error {
        ResolutionError::Failed(reason) => record(error.code(), &[reason], origin),
        ResolutionError::ResolverMismatch { declared, .. } => {
            record(error.code(), &["resolver", declared], origin)
        }
        _ => record(
            error.code(),
            &["resolution policy", "of the schema"],
            origin,
        )
        .with_message(error.message()),
    }
}
