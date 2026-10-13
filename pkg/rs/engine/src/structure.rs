//! Per-scope structural rules compiled to counters and a small state
//! machine, so each child is checked as it arrives and minimums at scope
//! completion.

use std::collections::BTreeMap;

use crate::names::Sym;

/// Occurrence bounds for one child name; `max: None` is unbounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Occurs {
    pub name: Sym,
    pub min: u64,
    pub max: Option<u64>,
}

/// The ordering rule of a document's root scope: the first active node
/// must be one of `first` when it is given; every later node must be one
/// of `following`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OrderRule {
    pub first: Option<Vec<Sym>>,
    pub following: Vec<Sym>,
}

/// The compiled rules of one scope: a counter per permitted child name,
/// exactly-one groups over those counters, and an optional order rule.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeRules {
    counters: Vec<Occurs>,
    by_name: BTreeMap<Sym, usize>,
    groups: Vec<Vec<usize>>,
    order: Option<OrderRule>,
}

/// The mutable state of one open scope: one count per counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeState {
    counts: Vec<u64>,
    admitted: u64,
}

/// A structural rule a child or a completed scope broke.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Violation {
    /// No field binding consumes the child and no children rule names it.
    NotPermitted {
        child: Sym,
    },
    TooMany {
        child: Sym,
        max: u64,
    },
    TooFew {
        child: Sym,
        count: u64,
        min: u64,
    },
    /// More than one member of an exactly-one group appeared.
    GroupRepeated {
        group: usize,
        child: Sym,
    },
    /// No member of an exactly-one group appeared.
    GroupMissing {
        group: usize,
    },
    /// The first node is not one the order rule lists.
    FirstNotAllowed {
        child: Sym,
    },
    /// A node after the first is not one the order rule lists.
    NotFollowing {
        child: Sym,
    },
    /// The scope completed without the node the order rule requires.
    FirstMissing,
}

impl Violation {
    /// The registered diagnostic code a binder reports this under.
    pub fn code(&self) -> &'static str {
        match self {
            Violation::NotPermitted { .. }
            | Violation::FirstNotAllowed { .. }
            | Violation::NotFollowing { .. } => "KDLITH-BND001",
            _ => "KDLITH-BND002",
        }
    }
}

impl ScopeRules {
    /// Permit a child name with its bounds; the caller permits each name
    /// once.
    pub(crate) fn permit(&mut self, occurs: Occurs) {
        self.by_name.insert(occurs.name, self.counters.len());
        self.counters.push(occurs);
    }

    /// Add an exactly-one group; returns the names that are not permitted.
    pub(crate) fn group(&mut self, members: &[Sym]) -> Vec<Sym> {
        let mut counters = Vec::new();
        let mut missing = Vec::new();
        for member in members {
            match self.by_name.get(member) {
                Some(&at) => counters.push(at),
                None => missing.push(*member),
            }
        }
        if missing.is_empty() {
            self.groups.push(counters);
        }
        missing
    }

    pub(crate) fn order(&mut self, rule: OrderRule) {
        self.order = Some(rule);
    }

    pub fn counters(&self) -> &[Occurs] {
        &self.counters
    }

    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }

    pub fn order_rule(&self) -> Option<&OrderRule> {
        self.order.as_ref()
    }

    pub fn permits(&self, child: Sym) -> bool {
        self.by_name.contains_key(&child)
    }

    /// A fresh state for a newly opened scope.
    pub fn start(&self) -> ScopeState {
        ScopeState {
            counts: vec![0; self.counters.len()],
            admitted: 0,
        }
    }

    /// Count one arriving child. The first violation it causes is
    /// returned; the child is not counted then.
    pub fn admit(&self, state: &mut ScopeState, child: Sym) -> Result<(), Violation> {
        if let Some(order) = &self.order {
            if state.admitted == 0 {
                if order
                    .first
                    .as_ref()
                    .is_some_and(|first| !first.contains(&child))
                {
                    return Err(Violation::FirstNotAllowed { child });
                }
            } else if !order.following.contains(&child) {
                return Err(Violation::NotFollowing { child });
            }
        }
        let Some(&at) = self.by_name.get(&child) else {
            return Err(Violation::NotPermitted { child });
        };
        let occurs = self.counters[at];
        let count = state.counts[at].saturating_add(1);
        if let Some(max) = occurs.max
            && count > max
        {
            return Err(Violation::TooMany { child, max });
        }
        for (group, members) in self.groups.iter().enumerate() {
            if members.contains(&at) && members.iter().any(|m| state.counts[*m] > 0) {
                return Err(Violation::GroupRepeated { group, child });
            }
        }
        state.counts[at] = count;
        state.admitted = state.admitted.saturating_add(1);
        Ok(())
    }

    /// Check minimums, exactly-one groups, and the required first node
    /// when the scope closes.
    pub fn complete(&self, state: &ScopeState) -> Vec<Violation> {
        let mut violations = Vec::new();
        if self.order.as_ref().is_some_and(|o| o.first.is_some()) && state.admitted == 0 {
            violations.push(Violation::FirstMissing);
        }
        for (at, occurs) in self.counters.iter().enumerate() {
            let count = state.counts[at];
            if count < occurs.min {
                violations.push(Violation::TooFew {
                    child: occurs.name,
                    count,
                    min: occurs.min,
                });
            }
        }
        for (group, members) in self.groups.iter().enumerate() {
            if members.iter().all(|m| state.counts[*m] == 0) {
                violations.push(Violation::GroupMissing { group });
            }
        }
        violations
    }
}
