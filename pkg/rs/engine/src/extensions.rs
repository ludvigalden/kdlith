//! The validators and context handlers a processor installs. A definition
//! naming one that is not installed fails to compile.

use std::collections::BTreeSet;

use kdlith_runtime::Handlers;

/// The validator every type expression is checked by.
pub const TYPE_EXPRESSION_VALIDATOR: &str = "type-expression";

/// The required version of [`TYPE_EXPRESSION_VALIDATOR`].
pub const TYPE_EXPRESSION_VERSION: &str = "0";

/// The binding codec the engine itself implements. A definition naming
/// a known codec the reading does not install still compiles; the binder
/// emits that payload as a pending typed-binding item.
pub const ARGUMENT_TAIL_CODEC: &str = "argument-tail";

/// The source-metadata provider the engine itself implements: the file
/// name's stem, without extension.
pub const PATH_STEM_SOURCE: &str = "path-stem";

/// Installed validators (by name and version), context handlers, binding
/// codecs, contribution resolvers, and source-metadata providers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    validators: BTreeSet<(String, String)>,
    handlers: BTreeSet<String>,
    binding_codecs: BTreeSet<String>,
    resolvers: BTreeSet<String>,
    source_providers: BTreeSet<String>,
}

impl Extensions {
    /// Nothing installed; the compiler refuses to run without the
    /// type-expression validator.
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine's own validators, binding codecs, and source-metadata
    /// providers: the `type-expression` validator version `0`, the
    /// `argument-tail` binding codec, and the `path-stem` source. The
    /// `fixture-value` codec is registered by the processor that
    /// implements it, and is deferred as a pending binding where none
    /// does; contribution resolvers are domain process business and are
    /// never installed here.
    pub fn standard() -> Self {
        Self::new()
            .with_validator(TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION)
            .with_binding_codec(ARGUMENT_TAIL_CODEC)
            .with_source_provider(PATH_STEM_SOURCE)
    }

    pub fn with_validator(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.validators.insert((name.into(), version.into()));
        self
    }

    pub fn with_handler(mut self, name: impl Into<String>) -> Self {
        self.handlers.insert(name.into());
        self
    }

    /// The handlers a processor installed, by name: compilation admits
    /// exactly the names the reading can execute, so a definition and
    /// its handler registry admit one surface, never two that drift.
    #[must_use]
    pub fn with_installed_handlers(mut self, handlers: &Handlers) -> Self {
        for name in handlers.names() {
            self.handlers.insert(name.to_owned());
        }
        self
    }

    pub fn with_binding_codec(mut self, name: impl Into<String>) -> Self {
        self.binding_codecs.insert(name.into());
        self
    }

    /// Register a contribution resolver the processor installs. The
    /// engine never resolves contributions itself; it routes the
    /// preserved records to the resolver the schema's policy names.
    pub fn with_resolver(mut self, name: impl Into<String>) -> Self {
        self.resolvers.insert(name.into());
        self
    }

    /// Register a source-metadata provider the processor installs.
    pub fn with_source_provider(mut self, name: impl Into<String>) -> Self {
        self.source_providers.insert(name.into());
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

    /// Whether a contribution resolver is installed.
    pub fn has_resolver(&self, name: &str) -> bool {
        self.resolvers.contains(name)
    }

    /// Whether a source-metadata provider is installed.
    pub fn has_source_provider(&self, name: &str) -> bool {
        self.source_providers.contains(name)
    }
}
