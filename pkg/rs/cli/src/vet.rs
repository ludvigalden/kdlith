//! `kdlith vet dialect|schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use std::sync::Arc;

use kdlith_engine::{
    AliasRoot, CheckScope, DataReader, Definition, EntryContext, Extensions, Handlers, LAYERED,
    Layered, LinkInputs, LinkReport, ReadOptions, SharedResolver, link_reading,
};
use kdlith_model::{Item, SourceId};
use kdlith_superproc::{load_dialect, load_schema_anchored, seed};

use crate::handlers;
use crate::report::{print_notes, report_all};
use crate::{DiagnosticsArg, ScopeArg, VetCommand};

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
            Ok(options) => {
                let options = options
                    .with_dialect(args.dialect)
                    .with_scope(scope_of(args.scope));
                match args.diagnostics {
                    None => vet_data(args.schema.as_deref(), &paths(&args.files), &options),
                    Some(DiagnosticsArg::Json) => vet_data_diagnostic_items(
                        args.schema.as_deref(),
                        &paths(&args.files),
                        &options,
                    ),
                }
            }
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
            print_notes(&definition.notes);
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
    max_invocations: Option<u32>,
    aliases: Vec<(String, PathBuf)>,
    match_root: Option<PathBuf>,
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
            max_invocations: None,
            aliases: Vec::new(),
            match_root: None,
        }
    }

    /// The same options anchoring profile match rules at `root`, or at
    /// the current directory when it is `None`.
    #[must_use]
    pub fn with_match_root(mut self, root: Option<PathBuf>) -> Self {
        self.match_root = root;
        self
    }

    /// The same options registering each `(name, directory)` as an
    /// import alias, in place of any registered before.
    #[must_use]
    pub fn with_aliases(mut self, aliases: Vec<(String, PathBuf)>) -> Self {
        self.aliases = aliases;
        self
    }

    /// The same options bounding one reading at `max` invocations,
    /// entries and imports together, or at the engine's default when it
    /// is `None`.
    #[must_use]
    pub fn with_max_invocations(mut self, max: Option<u32>) -> Self {
        self.max_invocations = max;
        self
    }

    /// The same options checking the reading under `scope`: complete
    /// links the reading's references after binding, local does not.
    /// Only the data verbs that link a reading (`vet data`, `inspect
    /// data`, and `export`) carry a scope.
    #[must_use]
    pub fn with_scope(mut self, scope: CheckScope) -> Self {
        self.scope = scope;
        self
    }

    /// The checking scope these options carry.
    pub(crate) fn scope(&self) -> CheckScope {
        self.scope
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

    /// The reader options: the named profile or profile set, the
    /// supplied anchors and import aliases, the match root, and the
    /// invocation bound.
    pub(crate) fn read_options(&self) -> ReadOptions {
        let mut options = ReadOptions::new().with_profiles(self.profiles.iter().cloned());
        if let Some(profile) = &self.profile {
            options = options.with_profile(profile.clone());
        }
        if let Some(max) = self.max_invocations {
            options = options.with_max_invocations(max as usize);
        }
        for (name, path) in &self.anchors {
            options = options.with_anchor(name.clone(), path.clone());
        }
        for (name, dir) in &self.aliases {
            options = options.with_alias(name.clone(), AliasRoot::Fs(dir.clone()));
        }
        if let Some(root) = &self.match_root {
            options = options.with_match_root(root.clone());
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

/// [`vet_data`], then each link diagnostic written to stdout as a
/// diagnostic item, in the link report's order, after the stream. The
/// stderr text and the exit are those `vet_data` decides; a failed
/// write fails the run.
fn vet_data_diagnostic_items(
    schema: Option<&Path>,
    entries: &[&Path],
    options: &DataOptions,
) -> ExitStatus {
    stream_data_then(
        schema,
        entries,
        options,
        false,
        write_item,
        |_, _, linked, status| {
            let Some(linked) = linked else {
                return status;
            };
            let mut out = std::io::stdout().lock();
            for record in &linked.report.diagnostics {
                let item = Item::Diagnostic(Box::new(record.clone()));
                if write_item(&mut out, &item).is_err() {
                    return ExitStatus::Failure;
                }
            }
            if std::io::Write::flush(&mut out).is_err() {
                return ExitStatus::Failure;
            }
            status
        },
    )
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
    stream_data_then(
        schema,
        entries,
        options,
        false,
        render,
        |_, _, _, status| status,
    )
}

/// One reading's link report, with each document's path as the reading
/// shows it.
pub(crate) struct Linked<'a> {
    pub(crate) report: &'a LinkReport,
    pub(crate) documents: &'a BTreeMap<SourceId, String>,
}

/// [`stream_data`], then `finish` with the definition the data bound
/// under, or `None` when it did not load, the context each entry
/// document's root invocation left, in entry order, the reading's link
/// report when it was linked, and the exit decided so far. A reading
/// that completed is linked under complete scope, or under any scope
/// when `report_links` asks for the report itself; its link diagnostics
/// reach stderr only, after every item, and decide the exit with the
/// binding diagnostics under complete scope alone.
pub(crate) fn stream_data_then(
    schema: Option<&Path>,
    entries: &[&Path],
    options: &DataOptions,
    report_links: bool,
    mut render: impl FnMut(&mut std::io::StdoutLock<'static>, &Item) -> std::io::Result<()>,
    finish: impl FnOnce(
        Option<&Definition>,
        &[EntryContext],
        Option<Linked<'_>>,
        ExitStatus,
    ) -> ExitStatus,
) -> ExitStatus {
    let extensions = options.extensions();
    let loaded = match schema {
        None => options.governing(&extensions),
        Some(file) => options.load_schema(file, &extensions),
    };
    let definition = match loaded {
        Ok(definition) => definition,
        Err(records) => return finish(None, &[], None, report_all(&records)),
    };
    let reader = options.reader(&definition);
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    let mut written = true;
    let gating = options.scope == CheckScope::Complete;
    let linking = gating || report_links;
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
    let report = (linking && read.is_ok()).then(|| link_reading(&definition, &inputs));
    let mut ungated: &[DiagnosticRecord] = &[];
    if let Some(report) = &report {
        if gating {
            records.extend(report.diagnostics.iter().cloned());
        } else {
            ungated = &report.diagnostics;
        }
    }
    let (status, contexts) = match read {
        Ok((_, contexts)) if written => {
            let status = report_all(&records);
            report_all(ungated);
            (status, contexts)
        }
        Ok((_, contexts)) => (ExitStatus::Failure, contexts),
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            (ExitStatus::Failure, Vec::new())
        }
    };
    let linked = report.as_ref().map(|report| Linked {
        report,
        documents: &inputs.documents,
    });
    finish(Some(&definition), &contexts, linked, status)
}

pub(crate) fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
