//! Processor-installed context handlers: the domain computation a
//! schema names inside an effects group and the engine runs at the
//! point the processing plan schedules it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use kdlith_model::Value;

use crate::context::ContextTarget;

/// What one handler call sees: the name the schema invoked it under,
/// the declared node it runs for, the target its effect group
/// declared, the node's fields as its timing knows them, and the slots
/// of the node's incoming context by name.
///
/// A handler declared for children sees header-known fields only; one
/// declared for following siblings sees the node's fully bound values.
/// Neither ever sees the node's own effects: a node binds against its
/// incoming context, and a late effect never changes nodes already
/// bound.
///
/// Constructed by the engine at the handler's site; read by the
/// implementation the processor installed, so its fields are part of
/// the contract rather than an internal representation.
#[derive(Clone, Copy, Debug)]
pub struct HandlerCall<'a> {
    pub handler: &'a str,
    /// The declared node's name as the schema spells it.
    pub node: &'a str,
    pub target: ContextTarget,
    pub fields: &'a BTreeMap<String, Value>,
    pub context: &'a BTreeMap<String, Value>,
}

/// One effect a handler's computation produced, naming its slot or
/// namespace the way the schema's own declarations do. The engine
/// resolves the names and validates the values exactly as it does a
/// declarative effect.
///
/// Constructed by the installed implementation, so its shape is the
/// handler contract rather than an internal representation.
#[derive(Clone, Debug, PartialEq)]
pub enum HandlerEffect {
    Set {
        slot: String,
        value: Value,
    },
    /// The slot becomes unset, not null.
    Remove {
        slot: String,
    },
    Clear {
        namespace: String,
    },
}

/// The computation one installed handler performs: pure domain logic
/// from the call's fields and context to the effects it produces. It
/// returns effects only; it never mutates shared state, and the engine
/// applies what it returns at the effect's declared transition point.
pub type HandlerFn = Arc<dyn Fn(&HandlerCall<'_>) -> Vec<HandlerEffect> + Send + Sync>;

/// The context handlers a processor installs, by the name a schema
/// names them under. Compilation admits a handler name only when the
/// reading installed it, and the binder fails a step whose handler the
/// registry lacks instead of silently omitting its behavior — a
/// handler mechanism never degrades a missing installation into a
/// silent pass.
#[derive(Clone, Default)]
pub struct Handlers {
    installed: BTreeMap<String, HandlerFn>,
}

impl Handlers {
    /// No handlers installed: every handler-named effect fails at its
    /// site under this registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Install one handler under the name a schema names it.
    pub fn install(&mut self, name: impl Into<String>, handler: HandlerFn) -> &mut Self {
        self.installed.insert(name.into(), handler);
        self
    }

    /// Install one handler, builder-style.
    #[must_use]
    pub fn with_handler(mut self, name: impl Into<String>, handler: HandlerFn) -> Self {
        self.install(name, handler);
        self
    }

    /// The installed handler a schema names, cloned cheaply.
    pub fn get(&self, name: &str) -> Option<HandlerFn> {
        self.installed.get(name).cloned()
    }

    /// Every installed name, in name order: exactly what a compilation
    /// may admit.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.installed.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.installed.is_empty()
    }
}

impl fmt::Debug for Handlers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.names()).finish()
    }
}
