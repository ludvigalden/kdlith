//! Contribution resolution: the preserved records a reading hands the
//! registered resolver, the policy it applies, the resolver contract,
//! and `layered`, the resolver kdlith ships.

use std::collections::BTreeMap;

use kdlith_model::{
    BoundContribution, DuplicateTreatment, Entry, MissingRequirement, OriginId, QualifiedName,
    Reference, ResolutionSelection, ResolvedContribution, Value,
};

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

/// The declared policy a resolver applies, as descriptors: the layer
/// names in precedence order (highest first; empty when the schema
/// declares none), per-target selection, duplicate treatment, and the
/// missing-target requirement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContributionPolicy {
    pub layers: Vec<String>,
    pub selection: ResolutionSelection,
    pub duplicates: DuplicateTreatment,
    pub missing: MissingRequirement,
}

impl ContributionPolicy {
    pub fn new(
        layers: Vec<String>,
        selection: ResolutionSelection,
        duplicates: DuplicateTreatment,
        missing: MissingRequirement,
    ) -> Self {
        Self {
            layers,
            selection,
            duplicates,
            missing,
        }
    }
}

/// The configuration resolver a processor registers: the one place
/// contribution resolution happens. The engine routes records and policy
/// here; it never implements this trait itself.
pub trait ContributionResolver: Send + Sync {
    /// The resolver's registered name; it must match the policy's.
    fn name(&self) -> &str;

    /// Resolve `records`, given in reading order, under `policy`. A
    /// refusal is a descriptive reason naming the target and layer,
    /// never quoting a contributed value.
    fn resolve(
        &self,
        policy: &ContributionPolicy,
        records: &[ContributionRecord],
    ) -> Result<Vec<ResolvedContribution>, String>;
}

/// The registered name of [`Layered`].
pub const LAYERED: &str = "layered";

/// The shipped resolver: declared layer precedence, `replace` or
/// `union` selection, `reject` or `last-wins` duplicates, and `reject`
/// or `allow` for referenced targets no record contributes. Targets are
/// resolved in sorted order and ties keep reading order, so equal
/// inputs always give the same output.
#[derive(Clone, Copy, Debug, Default)]
pub struct Layered;

/// A target's identity for grouping and ordering: domain, name, and
/// qualifiers. The written spelling is not part of it.
type TargetKey = (String, QualifiedName, Vec<(String, String)>);

fn key_of(target: &Reference) -> TargetKey {
    (
        target.domain.clone(),
        target.name.clone(),
        target.qualifiers.clone(),
    )
}

/// A target as a refusal names it.
fn shown(target: &Reference) -> String {
    format!("{} {}", target.domain, target.written)
}

fn in_layer(layer: Option<&str>) -> String {
    match layer {
        Some(layer) => format!("layer {layer}"),
        None => "no layer".to_owned(),
    }
}

impl Layered {
    /// The precedence rank of a record, 0 highest. Without declared
    /// layers every record shares one rank; with them, a layer-less
    /// record ranks below every declared layer and an undeclared layer
    /// is refused.
    fn rank(policy: &ContributionPolicy, record: &ContributionRecord) -> Result<usize, String> {
        if policy.layers.is_empty() {
            return Ok(0);
        }
        match &record.layer {
            None => Ok(policy.layers.len()),
            Some(layer) => policy
                .layers
                .iter()
                .position(|declared| declared == layer)
                .ok_or_else(|| {
                    format!(
                        "a contribution to {} is in layer {layer}, which the policy does not declare",
                        shown(&record.target)
                    )
                }),
        }
    }

    /// One target's records, at most one per rank, highest rank first.
    fn settle<'r>(
        policy: &ContributionPolicy,
        group: &[(usize, &'r ContributionRecord)],
    ) -> Result<Vec<&'r ContributionRecord>, String> {
        let mut by_rank: BTreeMap<usize, &ContributionRecord> = BTreeMap::new();
        for (rank, record) in group {
            if let Some(held) = by_rank.insert(*rank, record)
                && policy.duplicates == DuplicateTreatment::Reject
            {
                let layer = if policy.layers.is_empty() {
                    "one rank (the policy declares no layers)".to_owned()
                } else {
                    in_layer(held.layer.as_deref())
                };
                return Err(format!(
                    "{} is contributed more than once in {layer}",
                    shown(&record.target)
                ));
            }
        }
        Ok(by_rank.into_values().collect())
    }

    /// The resolved value of one target from its settled records,
    /// highest first.
    fn select(policy: &ContributionPolicy, settled: &[&ContributionRecord]) -> Value {
        let all_maps = settled.iter().all(|r| matches!(r.value, Value::Map(_)));
        if policy.selection != ResolutionSelection::Union || !all_maps {
            return settled[0].value.clone();
        }
        let mut merged: Vec<Entry> = Vec::new();
        for record in settled.iter().rev() {
            let Value::Map(entries) = &record.value else {
                continue;
            };
            for entry in entries {
                match merged.iter_mut().find(|held| held.key == entry.key) {
                    Some(held) => held.value = entry.value.clone(),
                    None => merged.push(entry.clone()),
                }
            }
        }
        Value::Map(merged)
    }

    /// The layers whose records reached a value, highest first: the
    /// winner alone under replacement, every settled record under union.
    fn layers_of(policy: &ContributionPolicy, settled: &[&ContributionRecord]) -> Vec<String> {
        let all_maps = settled.iter().all(|r| matches!(r.value, Value::Map(_)));
        let reached = if policy.selection == ResolutionSelection::Union && all_maps {
            settled
        } else {
            &settled[..1]
        };
        let mut layers: Vec<String> = Vec::new();
        for record in reached {
            if let Some(layer) = &record.layer
                && !layers.contains(layer)
            {
                layers.push(layer.clone());
            }
        }
        layers
    }
}

/// Every reference a value holds, in value order.
fn references<'v>(value: &'v Value, out: &mut Vec<&'v Reference>) {
    match value {
        Value::Reference(reference) => out.push(reference),
        Value::List(items) => items.iter().for_each(|item| references(item, out)),
        Value::Map(entries) => entries
            .iter()
            .for_each(|entry| references(&entry.value, out)),
        _ => {}
    }
}

impl ContributionResolver for Layered {
    fn name(&self) -> &str {
        LAYERED
    }

    fn resolve(
        &self,
        policy: &ContributionPolicy,
        records: &[ContributionRecord],
    ) -> Result<Vec<ResolvedContribution>, String> {
        let mut groups: BTreeMap<TargetKey, Vec<(usize, &ContributionRecord)>> = BTreeMap::new();
        for record in records {
            let rank = Self::rank(policy, record)?;
            groups
                .entry(key_of(&record.target))
                .or_default()
                .push((rank, record));
        }
        if policy.missing == MissingRequirement::Reject {
            // A target is a contributed domain's member; a reference into
            // a domain no record contributes to is a link, not a target.
            for record in records {
                let mut held = Vec::new();
                references(&record.value, &mut held);
                for reference in held {
                    let contributed = groups.keys().any(|key| key.0 == reference.domain);
                    if contributed && !groups.contains_key(&key_of(reference)) {
                        return Err(format!(
                            "{} is referenced by a contribution to {} in {}, but no record contributes it",
                            shown(reference),
                            shown(&record.target),
                            in_layer(record.layer.as_deref()),
                        ));
                    }
                }
            }
        }
        let mut resolved = Vec::with_capacity(groups.len());
        for group in groups.values() {
            let settled = Self::settle(policy, group)?;
            resolved.push(ResolvedContribution::new(
                group[0].1.target.clone(),
                Self::select(policy, &settled),
                Self::layers_of(policy, &settled),
            ));
        }
        Ok(resolved)
    }
}
