//! Engine-managed context: typed slots, generic effects, and scopes that
//! restore the caller's context on leave.

use std::fmt;

use kdlith_model::{OriginId, Value};

/// A context slot, numbered by the [`ContextLayout`] that declared it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(u32);

/// A slot namespace, the unit a `clear` effect empties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceId(u32);

impl SlotId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl NamespaceId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The slots and namespaces a compiled schema declares. What a slot means
/// is the schema's business; the layout only numbers them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextLayout {
    namespace_of: Vec<Option<NamespaceId>>,
    members: Vec<Vec<SlotId>>,
}

impl ContextLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a namespace.
    pub fn namespace(&mut self) -> NamespaceId {
        let id = NamespaceId(index_u32(self.members.len()));
        self.members.push(Vec::new());
        id
    }

    /// Declare a slot, optionally inside a namespace. Returns `None` when
    /// the namespace belongs to another layout.
    pub fn slot(&mut self, namespace: Option<NamespaceId>) -> Option<SlotId> {
        let id = SlotId(index_u32(self.namespace_of.len()));
        if let Some(namespace) = namespace {
            self.members.get_mut(namespace.index())?.push(id);
        }
        self.namespace_of.push(namespace);
        Some(id)
    }

    pub fn slot_count(&self) -> usize {
        self.namespace_of.len()
    }

    pub fn namespace_of(&self, slot: SlotId) -> Option<NamespaceId> {
        self.namespace_of.get(slot.index()).copied().flatten()
    }

    /// The slots of a namespace in declaration order.
    pub fn members(&self, namespace: NamespaceId) -> &[SlotId] {
        self.members
            .get(namespace.index())
            .map_or(&[], Vec::as_slice)
    }
}

/// A value held in a slot, with the origin of the binding that set it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SlotValue {
    pub value: Value,
    pub origin: OriginId,
}

impl SlotValue {
    pub fn new(value: Value, origin: OriginId) -> Self {
        Self { value, origin }
    }
}

/// A generic context operation. `Remove` leaves the slot unset, which is
/// distinct from setting it to null.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextEffect {
    Set { slot: SlotId, value: SlotValue },
    Remove { slot: SlotId },
    Clear { namespace: NamespaceId },
}

/// Where a group of effects applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ContextTarget {
    /// The siblings after the node, once it completes successfully.
    FollowingSiblings,
    /// The node's children, from the start of its children block.
    Children,
}

/// An effect or scope operation the context cannot carry out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextError {
    /// A slot outside this context's layout.
    UnknownSlot(SlotId),
    /// A namespace outside this context's layout.
    UnknownNamespace(NamespaceId),
    /// A scope left out of order, or left twice.
    ScopeOrder,
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextError::UnknownSlot(slot) => write!(f, "unknown context slot {}", slot.0),
            ContextError::UnknownNamespace(namespace) => {
                write!(f, "unknown context namespace {}", namespace.0)
            }
            ContextError::ScopeOrder => f.write_str("context scope left out of order"),
        }
    }
}

impl std::error::Error for ContextError {}

/// An entered scope. Leaving it restores every slot to its value at
/// entry; scopes nest and are left in reverse order.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a scope must be left to restore the caller's context"]
pub struct Scope {
    mark: usize,
    epoch: u64,
    outer: u64,
}

/// One slot value saved for restoration when its scope is left.
#[derive(Debug)]
struct Saved {
    slot: SlotId,
    value: Option<SlotValue>,
    /// The epoch in which the slot was last saved before this entry.
    epoch: u64,
}

/// The current values of a layout's slots, with an undo trail for open
/// scopes. A slot is saved at most once per open scope, so the trail is
/// bounded by slots times open scopes, never by document length.
#[derive(Debug)]
pub struct Context {
    layout: ContextLayout,
    slots: Vec<Option<SlotValue>>,
    saved_in: Vec<u64>,
    trail: Vec<Saved>,
    epoch: u64,
    next_epoch: u64,
}

impl Context {
    /// An empty context: every slot unset.
    pub fn new(layout: ContextLayout) -> Self {
        let count = layout.slot_count();
        let mut slots = Vec::with_capacity(count);
        slots.resize_with(count, || None);
        Self {
            layout,
            slots,
            saved_in: vec![0; count],
            trail: Vec::new(),
            epoch: 0,
            next_epoch: 1,
        }
    }

    pub fn layout(&self) -> &ContextLayout {
        &self.layout
    }

    /// The slot's value, or `None` when it is unset.
    pub fn get(&self, slot: SlotId) -> Option<&SlotValue> {
        self.slots.get(slot.index()).and_then(Option::as_ref)
    }

    /// A fresh context over the same layout holding only the listed slots
    /// of this one: the explicit projection an import receives.
    pub fn project(&self, slots: &[SlotId]) -> Result<Context, ContextError> {
        let mut projected = Context::new(self.layout.clone());
        for &slot in slots {
            let value = self
                .slots
                .get(slot.index())
                .ok_or(ContextError::UnknownSlot(slot))?;
            projected.slots[slot.index()].clone_from(value);
        }
        Ok(projected)
    }

    /// Open a scope; effects applied inside it are undone by [`Context::leave`].
    pub fn enter(&mut self) -> Scope {
        let scope = Scope {
            mark: self.trail.len(),
            epoch: self.next_epoch,
            outer: self.epoch,
        };
        self.epoch = self.next_epoch;
        self.next_epoch += 1;
        scope
    }

    /// Close the innermost scope, restoring the context it was entered in.
    pub fn leave(&mut self, scope: Scope) -> Result<(), ContextError> {
        if scope.epoch != self.epoch || scope.mark > self.trail.len() {
            return Err(ContextError::ScopeOrder);
        }
        while self.trail.len() > scope.mark {
            if let Some(saved) = self.trail.pop() {
                let at = saved.slot.index();
                self.slots[at] = saved.value;
                self.saved_in[at] = saved.epoch;
            }
        }
        self.epoch = scope.outer;
        Ok(())
    }

    /// Apply one effect in the innermost open scope.
    pub fn apply(&mut self, effect: ContextEffect) -> Result<(), ContextError> {
        match effect {
            ContextEffect::Set { slot, value } => self.replace(slot, Some(value)),
            ContextEffect::Remove { slot } => self.replace(slot, None),
            ContextEffect::Clear { namespace } => {
                let count = self
                    .layout
                    .members
                    .get(namespace.index())
                    .ok_or(ContextError::UnknownNamespace(namespace))?
                    .len();
                for member in 0..count {
                    let slot = self.layout.members[namespace.index()][member];
                    if self.slots[slot.index()].is_some() {
                        self.replace(slot, None)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn replace(&mut self, slot: SlotId, value: Option<SlotValue>) -> Result<(), ContextError> {
        let at = slot.index();
        let current = self
            .slots
            .get_mut(at)
            .ok_or(ContextError::UnknownSlot(slot))?;
        let previous = std::mem::replace(current, value);
        if self.saved_in[at] != self.epoch {
            self.trail.push(Saved {
                slot,
                value: previous,
                epoch: self.saved_in[at],
            });
            self.saved_in[at] = self.epoch;
        }
        Ok(())
    }
}

/// Children effects of one node, computed from its header alone and
/// applied when its children block opens.
pub struct HeaderEffects<'a> {
    effects: &'a mut Vec<ContextEffect>,
}

/// Following effects of one node, applied to its later siblings only
/// after it completes successfully.
pub struct FollowingEffects<'a> {
    effects: &'a mut Vec<ContextEffect>,
}

impl<'a> HeaderEffects<'a> {
    pub(crate) fn new(effects: &'a mut Vec<ContextEffect>) -> Self {
        Self { effects }
    }

    pub fn target(&self) -> ContextTarget {
        ContextTarget::Children
    }

    pub fn push(&mut self, effect: ContextEffect) {
        self.effects.push(effect);
    }
}

impl<'a> FollowingEffects<'a> {
    pub(crate) fn new(effects: &'a mut Vec<ContextEffect>) -> Self {
        Self { effects }
    }

    pub fn target(&self) -> ContextTarget {
        ContextTarget::FollowingSiblings
    }

    pub fn push(&mut self, effect: ContextEffect) {
        self.effects.push(effect);
    }
}

/// Layout indices are bounded by the compiled schema, far below
/// `u32::MAX`; saturate rather than wrap if that ever fails.
fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
