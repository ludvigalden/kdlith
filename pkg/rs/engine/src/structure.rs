//! Per-scope structural rules compiled to counters and a small state
//! machine, so each child is checked as it arrives and minimums at scope
//! completion.

use std::collections::BTreeMap;

use crate::bindings::SequenceStep;
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
/// exactly-one groups over those counters, an optional order rule, the
/// ordered sequence immediate children must match, and occurrence bounds
/// over the owning node's whole subtree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeRules {
    counters: Vec<Occurs>,
    by_name: BTreeMap<Sym, usize>,
    groups: Vec<Vec<usize>>,
    order: Option<OrderRule>,
    /// The ordered sequence; `None` when the scope declares none.
    sequence: Option<Vec<SequenceStep>>,
    /// Occurrence bounds over the owning node's subtree, checked when
    /// this scope completes against the subtree totals its closing
    /// children bubbled up.
    descendants: Vec<Occurs>,
}

/// The mutable state of one open scope: one count per counter, the
/// ordered sequence's position, and the subtree totals bubbled up from
/// closed child scopes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeState {
    counts: Vec<u64>,
    admitted: u64,
    /// The ordered sequence's current step, and how often that step has
    /// matched so far.
    step: usize,
    step_count: u64,
    /// Occurrences seen anywhere below this scope, by name: this scope's
    /// own children plus everything its closed child scopes bubbled up.
    subtree: BTreeMap<Sym, u64>,
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
    /// A child the ordered sequence names arrived where the compiled
    /// sequence does not admit it: its step is exhausted or the child
    /// belongs to a later step only.
    SequenceRejected {
        child: Sym,
        step: usize,
    },
    /// The scope completed before the ordered sequence satisfied step
    /// `step`.
    SequenceIncomplete {
        step: usize,
    },
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

    /// Set the ordered sequence immediate children must match.
    pub(crate) fn set_sequence(&mut self, steps: Vec<SequenceStep>) {
        self.sequence = Some(steps);
    }

    /// Add an occurrence bound over the owning node's subtree.
    pub(crate) fn descendant(&mut self, occurs: Occurs) {
        self.descendants.push(occurs);
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

    pub fn sequence(&self) -> Option<&[SequenceStep]> {
        self.sequence.as_deref()
    }

    pub fn descendants(&self) -> &[Occurs] {
        &self.descendants
    }

    pub fn permits(&self, child: Sym) -> bool {
        self.by_name.contains_key(&child)
    }

    /// A fresh state for a newly opened scope.
    pub fn start(&self) -> ScopeState {
        ScopeState {
            counts: vec![0; self.counters.len()],
            admitted: 0,
            step: 0,
            step_count: 0,
            subtree: BTreeMap::new(),
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
        if let Some(steps) = &self.sequence
            && steps.iter().any(|step| step.alternatives.contains(&child))
        {
            match sequence_position(steps, state, child) {
                Some(at) if at == state.step => {
                    state.step_count = state.step_count.saturating_add(1);
                }
                Some(at) => {
                    state.step = at;
                    state.step_count = 1;
                }
                None => {
                    return Err(Violation::SequenceRejected {
                        child,
                        step: state.step,
                    });
                }
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
        let seen = state.subtree.get(&child).copied().unwrap_or(0);
        state.subtree.insert(child, seen.saturating_add(1));
        state.admitted = state.admitted.saturating_add(1);
        Ok(())
    }

    /// Check minimums, exactly-one groups, the required first node, and
    /// the ordered sequence and descendant bounds when the scope closes.
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
        if let Some(steps) = &self.sequence {
            // Walk the remaining steps from the current position: each
            // must have met its minimum, and none with a minimum may be
            // left unvisited.
            let mut at = state.step;
            if state.step_count < steps.get(at).map_or(0, |step| step.min) {
                violations.push(Violation::SequenceIncomplete { step: at });
            }
            at += 1;
            while at < steps.len() {
                if steps[at].min > 0 {
                    violations.push(Violation::SequenceIncomplete { step: at });
                }
                at += 1;
            }
        }
        for occurs in &self.descendants {
            let count = state.subtree.get(&occurs.name).copied().unwrap_or(0);
            if let Some(max) = occurs.max
                && count > max
            {
                violations.push(Violation::TooMany {
                    child: occurs.name,
                    max,
                });
            }
            if count < occurs.min {
                violations.push(Violation::TooFew {
                    child: occurs.name,
                    count,
                    min: occurs.min,
                });
            }
        }
        violations
    }

    /// The subtree totals a closing scope hands to its parent: its own
    /// children plus everything bubbled up from below.
    pub(crate) fn subtree(state: &ScopeState) -> &BTreeMap<Sym, u64> {
        &state.subtree
    }

    /// Merge a closed child scope's subtree totals into its parent's, so
    /// the declaring scope's completion sees occurrences from its whole
    /// subtree.
    pub(crate) fn bubble(parent: &mut ScopeState, child: &BTreeMap<Sym, u64>) {
        for (name, count) in child {
            let held = parent.subtree.get(name).copied().unwrap_or(0);
            parent.subtree.insert(*name, held + count);
        }
    }
}

/// The step a sequenced child continues from: the current step while it
/// still admits the child, otherwise the next step that names it, leaving
/// satisfied or exhausted steps behind.
fn sequence_position(steps: &[SequenceStep], state: &ScopeState, child: Sym) -> Option<usize> {
    let current = steps.get(state.step)?;
    if current.alternatives.contains(&child) && state.step_count < current.max {
        return Some(state.step);
    }
    let mut at = state.step + 1;
    loop {
        let step = steps.get(at)?;
        if step.alternatives.contains(&child) {
            return Some(at);
        }
        // A step the child does not name must be satisfiable empty
        // (min 0) for the child to belong to a later one; a step with a
        // minimum cannot be skipped.
        if step.min > 0 {
            return None;
        }
        at += 1;
    }
}
