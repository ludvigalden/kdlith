//! The validators and context handlers a processor installs. A definition
//! naming one that is not installed fails to compile.

use std::collections::BTreeSet;

/// The validator every type expression is checked by.
pub const TYPE_EXPRESSION_VALIDATOR: &str = "type-expression";

/// The required version of [`TYPE_EXPRESSION_VALIDATOR`].
pub const TYPE_EXPRESSION_VERSION: &str = "0";

/// Installed validators (by name and version) and context handlers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    validators: BTreeSet<(String, String)>,
    handlers: BTreeSet<String>,
}

impl Extensions {
    /// Nothing installed; the compiler refuses to run without the
    /// type-expression validator.
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine's own validators: `type-expression` version `0`.
    pub fn standard() -> Self {
        Self::new().with_validator(TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION)
    }

    pub fn with_validator(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.validators.insert((name.into(), version.into()));
        self
    }

    pub fn with_handler(mut self, name: impl Into<String>) -> Self {
        self.handlers.insert(name.into());
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
}
