//! The value-resolution hook: the static surface through which the
//! consuming program supplies computed-default values. The engine
//! picks no source and passes descriptors as data, never executed.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use kdlith_model::{Location, Value};

use crate::definition::ProviderDecl;
use crate::types::Sensitivity;

/// What one resolution call sees: where the value is needed, which
/// declared field it fills, and the descriptors the schema declared
/// for it.
///
/// Constructed by the engine when a bound node's field is absent and its
/// declared fallback is computed; read by the resolver the consuming
/// program registered, so its fields are part of the contract rather
/// than an internal representation.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ResolutionCall<'a> {
    /// The document the node was read from, as the reading shows it.
    pub source: &'a str,
    /// The node the field belongs to: its source and span.
    pub site: Location,
    /// The declared node's name as the schema spells it.
    pub node: &'a str,
    /// The field the value fills, as the schema spells it: the slot
    /// identity of the call.
    pub field: &'a str,
    /// The field's declared type, as the schema spells it. The value the
    /// resolver returns is decoded against it.
    pub ty: &'a str,
    /// The field's declared sensitivity. A `secret` value passes the
    /// output policy like any other secret field: redacted unless the
    /// policy explicitly says emit.
    pub sensitivity: Option<Sensitivity>,
    /// The computed default's dependencies, as declared, in order.
    pub dependencies: &'a [String],
    /// The declared secret providers the dependencies name, in
    /// dependency order: descriptor data only. Nothing in the engine
    /// executes a provider's command or joins its arguments.
    pub providers: &'a [&'a ProviderDecl],
    /// The slots of the node's incoming context, by name.
    pub context: &'a BTreeMap<String, Value>,
}

impl ResolutionCall<'_> {
    /// Whether the value fills a field declared `sensitivity=secret`.
    pub fn is_secret(&self) -> bool {
        self.sensitivity == Some(Sensitivity::Secret)
    }
}

/// Why a resolver could not supply a value. The engine reports it as
/// a loud binding error at the node (`KDLITH-BND009`); an unresolvable
/// value is never replaced by a silent fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unresolvable {
    reason: String,
}

impl Unresolvable {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// The reason, as the resolver stated it. It appears in the
    /// diagnostic verbatim, so it must not quote a secret value.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for Unresolvable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Unresolvable {}

/// The resolver a consuming program registers: one trait covering
/// secret values and non-secret context variables, told apart by the
/// call's declared sensitivity and provider descriptors.
pub trait ValueResolver: Send + Sync {
    /// Resolve the value `call` asks for, or say why it cannot.
    fn resolve(&self, call: &ResolutionCall<'_>) -> Result<Value, Unresolvable>;
}

/// A registered resolver, shared cheaply across every invocation a
/// reading binds.
pub type SharedResolver = Arc<dyn ValueResolver>;
