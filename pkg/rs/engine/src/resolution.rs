//! Contribution resolution: the preserved records, the declared policy,
//! and the routing surface that hands them to the registered resolver.
//! The engine never resolves contributions itself.

use kdlith_model::{BoundContribution, OriginId, Reference, Value};

use crate::definition::{Definition, ResolutionPolicyDecl};

/// Why a resolution run could not proceed. Both failures are loud: an
/// uninstalled resolver is rejected at compile time already, and a
/// resolver that fails reports its own reason.
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
            ResolutionError::Failed(_) => "KDLITH-DLC006",
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

/// One preserved contribution record, as the resolver receives it:
/// target, value, layer, application scope, and source origin. Records
/// stay unresolved — layer precedence and merge policy belong to the
/// resolver, never to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContributionRecord {
    pub target: Reference,
    pub value: Value,
    pub layer: Option<String>,
    pub scope: Option<String>,
    pub origin: OriginId,
}

impl ContributionRecord {
    /// The record a bound contribution item preserves.
    pub fn from_bound(contribution: &BoundContribution) -> Self {
        Self {
            target: contribution.target.clone(),
            value: contribution.value.clone(),
            layer: contribution.layer.clone(),
            scope: contribution.scope.clone(),
            origin: contribution.origin,
        }
    }
}

/// One resolved contribution, as a resolver reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedContribution {
    pub target: Reference,
    pub value: Value,
    /// The layers that contributed, in the policy's precedence order.
    pub layers: Vec<String>,
}

/// The configuration resolver a processor registers: the one place
/// contribution resolution happens. The engine routes records and policy
/// here; it never implements this trait itself.
pub trait ContributionResolver {
    /// The resolver's registered name; it must match the policy's.
    fn name(&self) -> &str;

    /// Resolve `records` under `policy`. The descriptors the policy
    /// carries — layer precedence, selection, duplicate treatment,
    /// missing-value requirement — are the resolver's contract.
    fn resolve(
        &self,
        policy: &ResolutionPolicyDecl,
        records: &[ContributionRecord],
    ) -> Result<Vec<ResolvedContribution>, String>;
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
        .resolve(policy, records)
        .map_err(ResolutionError::Failed)
}
