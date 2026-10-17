//! `kdlith vet dialect|schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use std::sync::Arc;

use kdlith_engine::{
    CheckScope, DataReader, Definition, EntryContext, Extensions, Handlers, LAYERED, Layered,
    LinkInputs, ReadOptions, SharedResolver, bind_references, derive_candidates, index_identities,
    link,
};
use kdlith_model::Item;
use kdlith_superproc::{load_dialect, load_schema_anchored, seed};

use crate::handlers;
use crate::report::report_all;
use crate::{ScopeArg, VetCommand};

/// Run a parsed `vet` subcommand.
pub(crate) fn run(command: VetCommand) -> ExitStatus {
    match command {
        VetCommand::Dialect(args) => vet(
            VetTarget::Dialect,
            None,
            &paths(&args.files),
            scope_of(args.scope),
            &[],
        ),
        VetCommand::Schema(args) => vet(
            VetTarget::Schema,
            args.dialect.as_deref(),
            &paths(&args.files),
            scope_of(args.scope),
            &args.anchors,
        ),
        VetCommand::Data(args) => match args.profiles.data_options(args.anchors, args.handlers) {
            Ok(options) => vet_data(
                args.schema.as_deref(),
                &paths(&args.files),
                &options
                    .with_dialect(args.dialect)
                    .with_scope(scope_of(args.scope)),
            ),
            Err(status) => status,
        },
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

pub(crate) fn scope_of(scope: ScopeArg) -> CheckScope {
    match scope {
        ScopeArg::Local => CheckScope::Local,
        ScopeArg::Complete => CheckScope::Complete,
    }
}

/// The governing dialect: the bundled canonical dialect, or the one
/// entry document an explicit `--dialect` names, itself loaded with
/// complete checking.
pub(crate) fn governing_dialect(
    dialect: Option<&Path>,
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    match dialect {
        Some(file) => load_dialect(&[file], extensions, CheckScope::Complete),
        None => seed(extensions),
    }
}

/// The definition level being vetted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VetTarget {
    Dialect,
    /// A schema definition, under the bundled dialect or the one given
    /// by an explicit `--dialect`.
    Schema,
    /// Data documents, bound against a schema loaded under a dialect.
    Data,
}

/// Vet the definition whose entry documents are `entries`. A schema is
/// vetted under the bundled dialect, or the dialect loaded from the one
/// document `dialect` names, which is itself vetted with complete
/// checking first; its import expressions expand against `anchors`.
/// A dialect root binds no discovery grammar, so its imports are
/// literal and it takes no anchors.
pub fn vet(
    target: VetTarget,
    dialect: Option<&Path>,
    entries: &[&Path],
    scope: CheckScope,
    anchors: &[(String, PathBuf)],
) -> ExitStatus {
    let extensions = definition_extensions();
    let loaded = match target {
        VetTarget::Dialect => load_dialect(entries, &extensions, scope),
        VetTarget::Schema => match governing_dialect(dialect, &extensions) {
            Ok(governing) => load_schema_anchored(&governing, entries, &extensions, scope, anchors),
            Err(records) => Err(records),
        },
        VetTarget::Data => {
            eprintln!("kdlith: vet data binds data documents through vet data");
            return ExitStatus::Usage;
        }
    };
    match loaded {
        Ok(definition) => {
            println!("{}", summary_line(&definition));
            ExitStatus::Success
        }
        Err(records) => report_all(&records),
    }
}

/// The extensions a schema loads under on every verb that reads a
/// definition: the standard set plus the shipped contribution resolver,
/// so a schema naming `layered` loads alike on the schema and data
/// verbs while any other resolver name still fails closed.
pub(crate) fn definition_extensions() -> Extensions {
    Extensions::standard().with_resolver(LAYERED)
}

fn summary_line(definition: &Definition) -> String {
    let declared = definition
        .types
        .iter()
        .filter(|(_, decl)| decl.site.is_some())
        .count();
    format!(
        "ok: {} {} version {} scope={} documents={} types={declared} nodes={} slots={} profiles={} digest={}",
        definition.kind.root_name(),
        definition.name,
        definition.version.as_deref().unwrap_or("-"),
        definition.scope.name(),
        definition.documents.len(),
        definition.bindings.len(),
        definition.slots.len(),
        definition.profiles.len(),
        definition.digest(),
    )
}

/// What one data reading supplies beyond its documents: the dialect
/// definition replacing the bundled one, the discovery anchors the host
/// passes on the command line, the profile or profile set the
/// invocation names, the built-in context handlers to install, the
/// value resolver `--resolve-from` loaded, and the checking scope. The
/// data verbs (`vet
/// data`, `inspect data`, `read data`, `list items`, `cache refresh`,
/// and `export`) share one of these, so every path observes the same
/// execution semantics.
#[derive(Default)]
pub struct DataOptions {
    dialect: Option<PathBuf>,
    anchors: Vec<(String, PathBuf)>,
    profile: Option<String>,
    profiles: Vec<String>,
    handlers: Vec<String>,
    resolver: Option<SharedResolver>,
    scope: CheckScope,
}

impl DataOptions {
    pub fn new(
        anchors: Vec<(String, PathBuf)>,
        profile: Option<String>,
        handlers: Vec<String>,
    ) -> Self {
        Self {
            dialect: None,
            anchors,
            profile,
            profiles: Vec::new(),
            handlers,
            resolver: None,
            scope: CheckScope::Complete,
        }
    }

    /// The same options checking the reading under `scope`: complete
    /// links the reading's references after binding, local does not.
    /// Only `vet data` and `inspect data` link a reading.
    #[must_use]
    pub fn with_scope(mut self, scope: CheckScope) -> Self {
        self.scope = scope;
        self
    }

    /// The same options resolving computed defaults through `resolver`,
    /// or leaving them unresolved when it is `None`.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Option<SharedResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// The same options admitting the listed profile set, in listed
    /// order; an empty list admits none. The engine resolves the set
    /// per entry document in its one shared selection order.
    #[must_use]
    pub fn with_profiles(mut self, profiles: Vec<String>) -> Self {
        self.profiles = profiles;
        self
    }

    /// The same options under the dialect the one document `dialect`
    /// names, or under the bundled dialect when it is `None`.
    #[must_use]
    pub fn with_dialect(mut self, dialect: Option<PathBuf>) -> Self {
        self.dialect = dialect;
        self
    }

    /// The governing dialect: the bundled one, or the one the options
    /// name, itself loaded with complete checking.
    pub(crate) fn governing(
        &self,
        extensions: &Extensions,
    ) -> Result<Definition, Vec<DiagnosticRecord>> {
        governing_dialect(self.dialect.as_deref(), extensions)
    }

    /// The schema `schema` names, loaded with complete checking under
    /// the governing dialect and these options' extensions, its import
    /// expressions expanded against the supplied anchors.
    pub(crate) fn load_schema(
        &self,
        schema: &Path,
        extensions: &Extensions,
    ) -> Result<Definition, Vec<DiagnosticRecord>> {
        let governing = self.governing(extensions)?;
        load_schema_anchored(
            &governing,
            &[schema],
            extensions,
            CheckScope::Complete,
            &self.anchors,
        )
    }

    /// The handler registry the reading installs. Names were validated
    /// against the built-in catalog at argument parse time.
    pub(crate) fn registry(&self) -> Handlers {
        handlers::registry(&self.handlers)
    }

    /// The data reader over `definition` with the installed handlers,
    /// the shipped contribution resolver, and, when one was loaded, the
    /// value resolver.
    pub(crate) fn reader<'d>(&self, definition: &'d Definition) -> DataReader<'d> {
        let reader = DataReader::with_handlers(definition, self.registry())
            .with_contribution_resolver(Arc::new(Layered));
        match &self.resolver {
            Some(resolver) => reader.with_resolver(resolver.clone()),
            None => reader,
        }
    }

    /// The extensions a definition loads under so the compile-time
    /// installed-name check and the runtime registry admit one surface:
    /// the installed handlers and the shipped contribution resolver.
    pub(crate) fn extensions(&self) -> Extensions {
        definition_extensions().with_installed_handlers(&self.registry())
    }

    /// The reader options: the named profile or profile set and the
    /// supplied anchors.
    pub(crate) fn read_options(&self) -> ReadOptions {
        let mut options = ReadOptions::new().with_profiles(self.profiles.iter().cloned());
        if let Some(profile) = &self.profile {
            options = options.with_profile(profile.clone());
        }
        for (name, path) in &self.anchors {
            options = options.with_anchor(name.clone(), path.clone());
        }
        options
    }
}

/// Bind each data document against `schema` under the governing
/// dialect — the bundled one, or the single document the options'
/// dialect names — or against the dialect itself when `schema` is
/// absent. The documents'
/// imports execute as invocations: each document's items follow a
/// document item, and imported records reach the stream with their own
/// source identity. Bound items go to stdout as JSON lines; diagnostics
/// go to stderr.
pub fn vet_data(schema: Option<&Path>, entries: &[&Path], options: &DataOptions) -> ExitStatus {
    stream_data(schema, entries, options, write_item)
}

/// [`vet_data`] with each item written by `render`, which may reframe
/// it or write nothing. Diagnostics are collected from every item
/// whatever `render` writes, so the exit is decided as `vet data`
/// decides it.
pub(crate) fn stream_data(
    schema: Option<&Path>,
    entries: &[&Path],
    options: &DataOptions,
    render: impl FnMut(&mut std::io::StdoutLock<'static>, &Item) -> std::io::Result<()>,
) -> ExitStatus {
    stream_data_then(schema, entries, options, render, |_, _, status| status)
}

/// [`stream_data`], then `finish` with the definition the data bound
/// under, or `None` when it did not load, the context each entry
/// document's root invocation left, in entry order, and the exit
/// decided so far. Under complete scope a reading that completed is
/// linked first, and its link diagnostics decide the exit with the
/// binding diagnostics; they reach stderr only, after every item.
pub(crate) fn stream_data_then(
    schema: Option<&Path>,
    entries: &[&Path],
    options: &DataOptions,
    mut render: impl FnMut(&mut std::io::StdoutLock<'static>, &Item) -> std::io::Result<()>,
    finish: impl FnOnce(Option<&Definition>, &[EntryContext], ExitStatus) -> ExitStatus,
) -> ExitStatus {
    let extensions = options.extensions();
    let loaded = match schema {
        None => options.governing(&extensions),
        Some(file) => options.load_schema(file, &extensions),
    };
    let definition = match loaded {
        Ok(definition) => definition,
        Err(records) => return finish(None, &[], report_all(&records)),
    };
    let reader = options.reader(&definition);
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    let mut written = true;
    let linking = options.scope == CheckScope::Complete;
    let mut inputs = LinkInputs::new();
    let mut sink = |item: Item| {
        if let Item::Diagnostic(record) = &item {
            records.push((**record).clone());
        }
        if linking {
            inputs.observe(&item);
        }
        written &= render(&mut out, &item).is_ok();
    };
    let read = reader.read_entry_contexts(entries, &options.read_options(), &mut sink);
    drop(out);
    if linking && read.is_ok() {
        records.extend(link_reading(&definition, &inputs));
    }
    let (status, contexts) = match read {
        Ok((_, contexts)) if written => (report_all(&records), contexts),
        Ok((_, contexts)) => (ExitStatus::Failure, contexts),
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            (ExitStatus::Failure, Vec::new())
        }
    };
    finish(Some(&definition), &contexts, status)
}

/// The diagnostics of linking one reading: its identity index, its
/// references bound to their declared reference types, the candidates
/// each one's lookup policy derives, and the linked outcomes.
fn link_reading(definition: &Definition, inputs: &LinkInputs) -> Vec<DiagnosticRecord> {
    let index = index_identities(definition, &inputs.items, &inputs.aliases);
    let bound = bind_references(definition, &inputs.items);
    let lookups = derive_candidates(&index, &bound);
    link(definition, &index, lookups, &inputs.documents).diagnostics
}

pub(crate) fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
