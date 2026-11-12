//! `kdlith vet schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use std::sync::Arc;

use kdlith_engine::{
    CheckScope, DataReader, Definition, EntryContext, Extensions, Handlers, LAYERED, Layered,
    LinkInputs, LinkReport, ReadOptions, SharedResolver, SourceTree, check_reading, link_reading,
};
use kdlith_model::{Item, SourceId};
use kdlith_schema_kdl::{load_meta_schema, load_schema_with, seed};

use crate::handlers;
use crate::report::{mirror_diagnostics_as_json, report_all};
use crate::schema_hint;
use crate::{DiagnosticsArg, ScopeArg, VetCommand};

/// Run a parsed `vet` subcommand.
pub(crate) fn run(command: VetCommand) -> ExitStatus {
    match command {
        VetCommand::Schema(args) => {
            let entries = paths(&args.files);
            match definition_target(&entries, args.meta_schema.as_deref()) {
                Ok(target) => vet(
                    target,
                    args.meta_schema.as_deref(),
                    &entries,
                    scope_of(args.scope),
                    args.locations.read_options(),
                ),
                Err(status) => status,
            }
        }
        VetCommand::Data(args) => {
            mirror_diagnostics_as_json(args.diagnostics.is_some_and(DiagnosticsArg::json));
            match args.profiles.data_options(args.locations, args.handlers) {
                Ok(options) => {
                    let options = options
                        .with_meta_schema(args.meta_schema)
                        .with_scope(scope_of(args.scope));
                    vet_data(args.schema.as_deref(), &paths(&args.files), &options)
                }
                Err(status) => status,
            }
        }
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

/// The governing meta-schema: the bundled canonical meta-schema, or the one
/// entry document an explicit `--meta-schema` names, itself loaded with
/// complete checking.
pub(crate) fn governing_meta_schema(
    meta_schema: Option<&Path>,
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    match meta_schema {
        Some(file) => load_meta_schema(&[file], extensions, CheckScope::Complete),
        None => seed(extensions),
    }
}

/// The definition the schema verbs read from `entries`: the meta-schema
/// when the first entry document's root is the schema named schema, any
/// other schema otherwise. The meta-schema is read against its own
/// canonical definition, so a `--meta-schema` replacement does not apply
/// to it (`KDLITH-CLI001`, exit 2).
pub(crate) fn definition_target(
    entries: &[&Path],
    meta_schema: Option<&Path>,
) -> Result<VetTarget, ExitStatus> {
    if !entries
        .first()
        .is_some_and(|entry| declares_meta_schema(entry))
    {
        return Ok(VetTarget::Schema);
    }
    if meta_schema.is_some() {
        return Err(crate::usage::report_invalid_argument(
            "--meta-schema does not apply to the meta-schema itself",
        ));
    }
    Ok(VetTarget::MetaSchema)
}

/// Whether the document at `path` opens with the meta-schema's root,
/// `schema schema`. A document that cannot be read or parsed declares no
/// meta-schema here; the loader reports it.
fn declares_meta_schema(path: &Path) -> bool {
    use std::io::Read;
    let mut text = String::new();
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    if file
        .take(kdlith_engine::MAX_DOCUMENT_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        return false;
    }
    let mut parser = kdlith_parse::Parser::new(&text);
    let mut root = false;
    while let Ok(Some(event)) = parser.next_event() {
        match event.item {
            kdlith_parse::Event::Node { name, .. } if !root => {
                if name != kdlith_engine::DefinitionKind::Schema.root_name() {
                    return false;
                }
                root = true;
            }
            kdlith_parse::Event::Entry(entry) if root && entry.name.is_none() => {
                return matches!(
                    entry.value,
                    kdlith_parse::Value::String(name) if name == kdlith_engine::META_SCHEMA_NAME
                );
            }
            kdlith_parse::Event::Entry(_) => {}
            _ => return false,
        }
    }
    false
}

/// The definition level being vetted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VetTarget {
    /// The meta-schema, the schema named schema, read against its own
    /// canonical definition.
    MetaSchema,
    /// A schema definition, under the bundled meta-schema or the one given
    /// by an explicit `--meta-schema`.
    Schema,
    /// Data documents, bound against a schema loaded under a meta-schema.
    Data,
}

/// Vet the definition whose entry documents are `entries`. A schema is
/// vetted under the bundled meta-schema, or the meta-schema loaded from the one
/// document `meta_schema` names, which is itself vetted with complete
/// checking first; its import locations resolve under `options`. A
/// meta-schema's imports are literal locations, so it takes none.
pub fn vet(
    target: VetTarget,
    meta_schema: Option<&Path>,
    entries: &[&Path],
    scope: CheckScope,
    options: ReadOptions,
) -> ExitStatus {
    let extensions = definition_extensions();
    let loaded = match target {
        VetTarget::MetaSchema => load_meta_schema(entries, &extensions, scope),
        VetTarget::Schema => match governing_meta_schema(meta_schema, &extensions) {
            Ok(governing) => load_schema_with(&governing, entries, &extensions, scope, options),
            Err(records) => Err(records),
        },
        VetTarget::Data => {
            eprintln!("kdlith: vet data binds data documents through vet data");
            return ExitStatus::Usage;
        }
    };
    match loaded {
        Ok(definition) => {
            let status = report_all(&definition.notes);
            println!("{}", summary_line(&definition));
            status
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

/// What one data reading supplies beyond its documents: the meta-schema
/// definition replacing the bundled one, the project root and named
/// sources the command line passes, the profile or profile set the
/// invocation names, the built-in context handlers to install, the
/// value resolver `--resolve-from` loaded, and the checking scope. The
/// data verbs (`vet
/// data`, `inspect data`, `read data`, `list items`, `cache refresh`,
/// and `export`) share one of these, so every path observes the same
/// execution semantics.
#[derive(Default)]
pub struct DataOptions {
    meta_schema: Option<PathBuf>,
    root: Option<PathBuf>,
    sources: Vec<(String, SourceTree)>,
    manifest: Option<PathBuf>,
    overlay: Option<PathBuf>,
    overrides: Vec<(String, PathBuf)>,
    store: Option<PathBuf>,
    profile: Option<String>,
    profiles: Vec<String>,
    handlers: Vec<String>,
    resolver: Option<SharedResolver>,
    scope: CheckScope,
    max_invocations: Option<u32>,
    threads: Option<u32>,
}

impl DataOptions {
    pub fn new(profile: Option<String>, handlers: Vec<String>) -> Self {
        Self {
            meta_schema: None,
            root: None,
            sources: Vec::new(),
            manifest: None,
            overlay: None,
            overrides: Vec::new(),
            store: None,
            profile,
            profiles: Vec::new(),
            handlers,
            resolver: None,
            scope: CheckScope::Complete,
            max_invocations: None,
            threads: None,
        }
    }

    /// The same options rooted at `root`, or at the current directory
    /// when it is `None`, registering each `(name, directory)` as a named
    /// source in place of any registered before.
    #[must_use]
    pub fn with_locations(
        mut self,
        root: Option<PathBuf>,
        sources: Vec<(String, PathBuf)>,
    ) -> Self {
        self.root = root;
        self.sources = sources
            .into_iter()
            .map(|(name, dir)| (name, SourceTree::dir(dir)))
            .collect();
        self
    }

    /// The same options reading the sources manifest `manifest` and the
    /// overrides file `overlay`, overriding each `(name, directory)`, and
    /// resolving origin sources from `store`.
    #[must_use]
    pub fn with_manifest(
        mut self,
        manifest: Option<PathBuf>,
        overlay: Option<PathBuf>,
        overrides: Vec<(String, PathBuf)>,
        store: Option<PathBuf>,
    ) -> Self {
        self.manifest = manifest;
        self.overlay = overlay;
        self.overrides = overrides;
        self.store = store;
        self
    }

    /// The same options registering one more named source, after those
    /// registered before.
    #[must_use]
    pub fn with_source(mut self, name: impl Into<String>, tree: SourceTree) -> Self {
        self.sources.push((name.into(), tree));
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

    /// The same options binding one reading on `threads` threads, or on
    /// one when it is `None`.
    #[must_use]
    pub fn with_threads(mut self, threads: Option<u32>) -> Self {
        self.threads = threads;
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

    /// The same options under the meta-schema the one document `meta_schema`
    /// names, or under the bundled meta-schema when it is `None`.
    #[must_use]
    pub fn with_meta_schema(mut self, meta_schema: Option<PathBuf>) -> Self {
        self.meta_schema = meta_schema;
        self
    }

    /// The governing meta-schema: the bundled one, or the one the options
    /// name, itself loaded with complete checking.
    pub(crate) fn governing(
        &self,
        extensions: &Extensions,
    ) -> Result<Definition, Vec<DiagnosticRecord>> {
        governing_meta_schema(self.meta_schema.as_deref(), extensions)
    }

    /// The schema `schema` names, loaded with complete checking under
    /// the governing meta-schema and these options' extensions, its import
    /// locations resolved under the root and sources.
    pub(crate) fn load_schema(
        &self,
        schema: &Path,
        extensions: &Extensions,
    ) -> Result<Definition, Vec<DiagnosticRecord>> {
        let governing = self.governing(extensions)?;
        load_schema_with(
            &governing,
            &[schema],
            extensions,
            CheckScope::Complete,
            self.locations(ReadOptions::new()),
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

    /// `options` rooted at the project root, with the named sources
    /// registered.
    fn locations(&self, mut options: ReadOptions) -> ReadOptions {
        if let Some(root) = &self.root {
            options = options.with_root(root.clone());
        }
        for (name, tree) in &self.sources {
            options = options.with_source(name.clone(), tree.clone());
        }
        if let Some(manifest) = &self.manifest {
            options = options.with_sources_manifest(manifest.clone());
        }
        if let Some(overlay) = &self.overlay {
            options = options.with_overrides_file(overlay.clone());
        }
        for (name, dir) in &self.overrides {
            options = options.with_override(name.clone(), dir.clone());
        }
        if let Some(store) = &self.store {
            options = options.with_store(store.clone());
        }
        options
    }

    /// The reader options: the named profile or profile set, the root
    /// and sources, and the invocation bound.
    pub(crate) fn read_options(&self) -> ReadOptions {
        let mut options = ReadOptions::new().with_profiles(self.profiles.iter().cloned());
        if let Some(profile) = &self.profile {
            options = options.with_profile(profile.clone());
        }
        if let Some(max) = self.max_invocations {
            options = options.with_max_invocations(max as usize);
        }
        if let Some(threads) = self.threads {
            options = options.with_threads(threads as usize);
        }
        self.locations(options)
    }
}

/// Bind each data document against `schema` under the governing
/// meta-schema — the bundled one, or the single document the options'
/// meta-schema names — or against the meta-schema itself when `schema` is
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
    render: impl FnMut(&mut Out, &Item) -> std::io::Result<()>,
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
    mut render: impl FnMut(&mut Out, &Item) -> std::io::Result<()>,
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
        Err(records) => {
            return schema_hint::on_failure(
                schema,
                entries,
                finish(None, &[], None, report_all(&records)),
            );
        }
    };
    let reader = options.reader(&definition);
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = stdout();
    let mut written = true;
    let gating = options.scope == CheckScope::Complete;
    let linking = gating || report_links;
    let mut inputs = LinkInputs::new(&definition);
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
    // Every item reaches stdout before any diagnostic reaches stderr.
    written &= std::io::Write::flush(&mut out).is_ok();
    drop(out);
    // Only a consumer of the report itself needs every link; the
    // verdict needs the failed ones alone.
    let report = (linking && read.is_ok()).then(|| {
        if report_links {
            link_reading(&inputs)
        } else {
            check_reading(&inputs)
        }
    });
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
    let status = finish(Some(&definition), &contexts, linked, status);
    schema_hint::on_failure(schema, entries, status)
}

/// Standard output for an item stream: locked for the run and written
/// in blocks, never a line at a time. The stream's writer flushes it
/// before anything is reported on standard error.
pub(crate) type Out = std::io::BufWriter<std::io::StdoutLock<'static>>;

/// How many bytes an item stream gathers between writes.
const OUT_BYTES: usize = 1 << 16;

pub(crate) fn stdout() -> Out {
    std::io::BufWriter::with_capacity(OUT_BYTES, std::io::stdout().lock())
}

pub(crate) fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
