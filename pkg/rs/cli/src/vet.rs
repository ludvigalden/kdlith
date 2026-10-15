//! `kdlith vet dialect|schema|data`: load and check a definition, or
//! bind data documents against one; diagnostics go to stderr. `vet data`
//! streams bound items to stdout as one JSON object per line.

use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_engine::{CheckScope, DataReader, Definition, Extensions, Handlers, ReadOptions};
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
        VetCommand::Data(args) => vet_data(
            args.schema.as_deref(),
            &paths(&args.files),
            &DataOptions::new(args.anchors, args.profile, args.handlers).with_dialect(args.dialect),
        ),
    }
}

fn paths(files: &[PathBuf]) -> Vec<&Path> {
    files.iter().map(PathBuf::as_path).collect()
}

fn scope_of(scope: ScopeArg) -> CheckScope {
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
    let extensions = Extensions::standard();
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
/// passes on the command line, the profile the invocation names, and
/// the built-in context handlers to install. The data verbs (`vet
/// data`, `inspect data`, `read data`, `list items`, `cache refresh`,
/// and `export`) share one of these, so every path observes the same
/// execution semantics.
#[derive(Default)]
pub struct DataOptions {
    dialect: Option<PathBuf>,
    anchors: Vec<(String, PathBuf)>,
    profile: Option<String>,
    handlers: Vec<String>,
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
            handlers,
        }
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

    /// The extensions a definition loads under so the compile-time
    /// installed-name check and the runtime registry admit one surface.
    pub(crate) fn extensions(&self) -> Extensions {
        Extensions::standard().with_installed_handlers(&self.registry())
    }

    /// The reader options: the named profile and the supplied anchors.
    pub(crate) fn read_options(&self) -> ReadOptions {
        let mut options = ReadOptions::new();
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
    let extensions = options.extensions();
    let loaded = match schema {
        None => options.governing(&extensions),
        Some(file) => options.load_schema(file, &extensions),
    };
    let definition = match loaded {
        Ok(definition) => definition,
        Err(records) => return report_all(&records),
    };
    let reader = DataReader::with_handlers(&definition, options.registry());
    let mut records: Vec<DiagnosticRecord> = Vec::new();
    let mut out = std::io::stdout().lock();
    let mut written = true;
    let mut sink = |item: Item| {
        if let Item::Diagnostic(record) = &item {
            records.push((**record).clone());
        }
        written &= write_item(&mut out, &item).is_ok();
    };
    let read = reader.read(entries, &options.read_options(), &mut sink);
    drop(out);
    match read {
        Ok(_) if written => report_all(&records),
        Ok(_) => ExitStatus::Failure,
        Err(error) => {
            report_all(&records);
            eprintln!("kdlith: {error}");
            ExitStatus::Failure
        }
    }
}

fn write_item(out: &mut impl std::io::Write, item: &Item) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}
