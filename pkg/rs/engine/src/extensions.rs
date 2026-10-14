//! The validators and context handlers a processor installs. A definition
//! naming one that is not installed fails to compile.

use std::collections::BTreeSet;

/// The validator every type expression is checked by.
pub const TYPE_EXPRESSION_VALIDATOR: &str = "type-expression";

/// The required version of [`TYPE_EXPRESSION_VALIDATOR`].
pub const TYPE_EXPRESSION_VERSION: &str = "0";

/// The binding codecs the engine itself implements, registered like
/// validators and handlers so naming one that is not installed fails
/// closed.
pub const ARGUMENT_TAIL_CODEC: &str = "argument-tail";

/// Installed validators (by name and version), context handlers, and
/// binding codecs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    validators: BTreeSet<(String, String)>,
    handlers: BTreeSet<String>,
    binding_codecs: BTreeSet<String>,
}

impl Extensions {
    /// Nothing installed; the compiler refuses to run without the
    /// type-expression validator.
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine's own validators and binding codecs: the
    /// `type-expression` validator version `0` and the `argument-tail`
    /// binding codec. The `fixture-value` codec is registered by the
    /// processor that implements it.
    pub fn standard() -> Self {
        Self::new()
            .with_validator(TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION)
            .with_binding_codec(ARGUMENT_TAIL_CODEC)
    }

    pub fn with_validator(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.validators.insert((name.into(), version.into()));
        self
    }

    pub fn with_handler(mut self, name: impl Into<String>) -> Self {
        self.handlers.insert(name.into());
        self
    }

    pub fn with_binding_codec(mut self, name: impl Into<String>) -> Self {
        self.binding_codecs.insert(name.into());
        self
    }

    /// Whether a validator is installed; an unversioned use accepts any
    /// installed version of the name.
    pub fn has_validator(&self, name: &str, version: Option<&str>) -> bool {
        self.validators
            .iter()
            .any(|(installed, at)| installed == name && version.is_none_or(|wanted| wanted == at))
    }

    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains(name)
    }

    /// Whether a binding codec is installed.
    pub fn has_binding_codec(&self, name: &str) -> bool {
        self.binding_codecs.contains(name)
    }
}
