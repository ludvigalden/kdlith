//! The descriptors of a declared contribution-resolution policy:
//! per-target selection, duplicate treatment, and the missing-target
//! requirement.

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
