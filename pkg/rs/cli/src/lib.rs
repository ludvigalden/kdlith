//! Commands, arguments, output conventions, and orchestration: a
//! verb-first clap command tree (gen, vet, export, inspect, list,
//! read, cache, changelog, plus help).
mod cache;
mod changelog;
mod export;
mod generate;
mod handlers;
mod inspect;
mod list;
mod read;
mod report;
mod resolve;
mod schema_hint;
mod sources;
mod usage;
mod vet;

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use kdlith_diagnostic::ExitStatus;
use kdlith_engine::ReadOptions;

pub use export::export;
pub use generate::{GenMode, GenTarget, generate};
pub use inspect::inspect;
pub use kdlith_engine::SourceTree;
pub use read::read;
pub use vet::{DataOptions, VetTarget, vet, vet_data};

/// The whole command line: one verb.
#[derive(Parser)]
#[command(
    name = "kdlith",
    bin_name = "kdlith",
    about = "Read typed KDL, validate definitions, and generate Rust processors",
    version,
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The verbs. There are no create, update, delete, or run verbs:
/// kdlith processes authored files, it is not an authored-content
/// lifecycle manager.
#[derive(Subcommand)]
enum Command {
    /// Generate a Rust processor crate from definitions
    Gen(GenArgs),
    /// Validate dialect definitions, schemas, or data
    Vet(VetArgs),
    /// Process data and write typed output
    Export(ExportArgs),
    /// Explain definitions, bindings, references, and provenance
    Inspect(InspectArgs),
    /// List declared types, nodes, profiles, or indexed items
    List(ListArgs),
    /// Read one definition or indexed data item
    Read(ReadArgs),
    /// Inspect, refresh, or clean derived processing state: cached
    /// results and record context snapshots
    Cache(CacheArgs),
    /// Report the sources a manifest declares: the fetch plan, the
    /// integrity of the trees the store holds, or every source with its
    /// digest
    Sources(SourcesArgs),
    /// Show the embedded changelog archive
    Changelog,
    /// Print help for a command
    Help(HelpArgs),
}

/// `kdlith help [command]...`.
#[derive(Args)]
struct HelpArgs {
    /// The command to print help for
    command: Vec<String>,
}

/// `kdlith gen dialect|schema|workspace`.
#[derive(Args)]
struct GenArgs {
    #[command(subcommand)]
    command: GenCommand,
}

#[derive(Subcommand)]
enum GenCommand {
    /// Generate the processor crate a dialect definition's documents
    /// compile to, embedding them as its seed
    Dialect(GenFormArgs),
    /// Generate the data reader a schema definition compiles to, under
    /// the canonical dialect
    Schema(Box<GenSchemaArgs>),
    /// Check the workspace root's kdlith dependencies against a named
    /// source and write the owned .cargo/config.toml that patches them
    /// to the source's local override
    Workspace(Box<GenWorkspaceArgs>),
}

#[derive(Args)]
struct GenFormArgs {
    /// The crate name the generated package publishes as
    #[arg(long = "crate", value_name = "NAME", required = true)]
    crate_name: String,
    /// The directory the generated crate is written to
    #[arg(long, value_name = "DIR", required = true)]
    out: PathBuf,
    /// Print the generator-owned paths a writing run would emit; write
    /// nothing
    #[arg(long)]
    dry_run: bool,
    /// Compare the generated tree against the destination without
    /// writing; report drift and exit nonzero on any difference. A
    /// definition the generator refuses fails before anything is compared
    #[arg(long, conflicts_with = "dry_run")]
    check: bool,
    /// Definition documents: the dialect entrypoint, or the schema's
    /// documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// The directory the generated manifest roots its kdlith path
    /// dependencies at, written verbatim (default: the sibling layout)
    #[arg(
        long = "kdlith-path",
        value_name = "PATH",
        conflicts_with_all = ["kdlith_version", "kdlith_workspace"],
        value_parser = handlers::parse_kdlith_path
    )]
    kdlith_path: Option<String>,
    /// The version the generated manifest requests the kdlith crates at
    /// from the registry, written verbatim
    #[arg(
        long = "kdlith-version",
        value_name = "VER",
        conflicts_with = "kdlith_workspace",
        value_parser = handlers::parse_kdlith_version
    )]
    kdlith_version: Option<String>,
    /// Name each kdlith dependency `{ workspace = true }` and inherit the
    /// workspace lints: the consuming workspace's root manifest names the
    /// kdlith source (see gen workspace)
    #[arg(long)]
    kdlith_workspace: bool,
}

#[derive(Args)]
struct GenSchemaArgs {
    #[command(flatten)]
    form: GenFormArgs,
    #[command(flatten)]
    locations: LocationArgs,
    /// A built-in context handler the schema may name, by name
    /// (repeatable); generation admits the name and never runs it
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
}

/// `kdlith gen workspace`.
#[derive(Args)]
struct GenWorkspaceArgs {
    #[command(flatten)]
    locations: LocationArgs,
    /// The source of the sources manifest the kdlith crates come from:
    /// its origin's git= and rev= pin them, and its override, if any,
    /// names the local checkout the patch points at
    #[arg(long, value_name = "NAME", default_value = "kdlith")]
    kdlith_source: String,
    /// Print the expected dependency lines and the patch file plan;
    /// write nothing
    #[arg(long)]
    dry_run: bool,
    /// Compare the root dependencies and the patch file without writing;
    /// report every difference and exit nonzero on any
    #[arg(long, conflicts_with = "dry_run")]
    check: bool,
}

/// `kdlith vet dialect|schema|data`.
#[derive(Args)]
struct VetArgs {
    #[command(subcommand)]
    command: VetCommand,
}

#[derive(Subcommand)]
enum VetCommand {
    /// Load a dialect definition with its imports, compile, link, and
    /// check it
    Dialect(VetDialectArgs),
    /// Vet a schema definition under the bundled dialect, or the one
    /// `--dialect` names, itself vetted with complete checking
    Schema(VetSchemaArgs),
    /// Bind data documents against the schema `--schema` names (or the
    /// dialect itself), under the bundled dialect or `--dialect`, then
    /// link their references; items go to stdout as JSON lines
    Data(VetDataArgs),
}

#[derive(Args)]
struct VetDialectArgs {
    /// Check the named documents alone (local) or also resolve imports,
    /// link references, and check self-description (complete)
    #[arg(long, value_enum, default_value_t = ScopeArg::Complete)]
    scope: ScopeArg,
    /// Definition documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(Args)]
struct VetSchemaArgs {
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// Check the named documents alone (local) or also resolve imports,
    /// link references, and check self-description (complete)
    #[arg(long, value_enum, default_value_t = ScopeArg::Complete)]
    scope: ScopeArg,
    #[command(flatten)]
    locations: LocationArgs,
    /// Schema definition documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(Args)]
struct VetDataArgs {
    /// A dialect definition document replacing the bundled one
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document; without one, data binds against
    /// the dialect itself
    #[arg(long = "schema", value_name = "FILE")]
    schema: Option<PathBuf>,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Bind the documents alone (local) or also link every reference a
    /// declared reference type looks up, failing on any that does not
    /// resolve to exactly one declaration (complete)
    #[arg(long, value_enum, default_value_t = ScopeArg::Complete)]
    scope: ScopeArg,
    /// Also write every diagnostic of the run to stdout as one compact
    /// JSON line per record, beside the unchanged stderr text; the exit
    /// is unchanged
    #[arg(long = "diagnostics", value_enum, value_name = "FORMAT")]
    diagnostics: Option<DiagnosticsArg>,
    /// Data documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// Where import locations resolve, on every verb that reads a schema or
/// data: the project root and the named sources.
#[derive(Args, Clone, Default)]
struct LocationArgs {
    /// The project root (default: the current directory): every location
    /// without a $name/ prefix stays inside it, and profile match rules
    /// read entry paths relative to it
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,
    /// The earlier spelling of --root, kept so existing invocations read
    /// the same root
    #[arg(long, value_name = "DIR", hide = true, conflicts_with = "root")]
    match_root: Option<PathBuf>,
    /// A named source, as NAME=DIR (repeatable): a location written
    /// $NAME/PATH resolves PATH from DIR, wherever the importing document
    /// lives, and never leaves DIR
    #[arg(long = "source", value_name = "NAME=DIR", value_parser = handlers::parse_source)]
    sources: Vec<(String, PathBuf)>,
    /// A sources manifest declaring sources by name: a directory, or an
    /// origin whose tree the store holds, optionally pinned by digest
    #[arg(long = "sources", value_name = "FILE")]
    manifest: Option<PathBuf>,
    /// An overrides file: override nodes only, above the manifest's own
    #[arg(long = "overrides", value_name = "FILE")]
    overlay: Option<PathBuf>,
    /// Replace a declared source with a directory, as NAME=DIR
    /// (repeatable), above every file's overrides; each override warns
    #[arg(long = "override", value_name = "NAME=DIR", value_parser = handlers::parse_source)]
    overrides: Vec<(String, PathBuf)>,
    /// The store origin sources resolve from; kdlith never fetches
    #[arg(long, value_name = "DIR")]
    store: Option<PathBuf>,
    /// Fail the run on any warning, an overridden source among them;
    /// notes, such as an unpinned source, never fail it
    #[arg(long)]
    warnings_as_errors: bool,
    /// The sources the host binary registers, after the command line's.
    #[arg(skip)]
    hosted: Vec<(String, SourceTree)>,
}

impl LocationArgs {
    /// The project root, however it was spelled.
    fn root(&self) -> Option<PathBuf> {
        self.root.clone().or_else(|| self.match_root.clone())
    }

    /// `options` rooted and sourced as these arguments and the host say.
    fn apply(&self, options: DataOptions) -> DataOptions {
        self.hosted
            .iter()
            .fold(
                options.with_locations(self.root(), self.sources.clone()),
                |options, (name, tree)| options.with_source(name.clone(), tree.clone()),
            )
            .with_manifest(
                self.manifest.clone(),
                self.overlay.clone(),
                self.overrides.clone(),
                self.store.clone(),
            )
    }

    /// Reader options carrying the root and the named sources, for a
    /// verb that reads only a schema.
    fn read_options(&self) -> ReadOptions {
        self.apply(DataOptions::new(None, Vec::new()))
            .read_options()
    }
}

/// The profile selection and value resolution every data verb shares:
/// one explicit profile, or a listed profile set (the two exclude each
/// other), and the one file computed defaults resolve from.
#[derive(Args, Clone)]
struct ProfileArgs {
    /// The profile the invocation binds; excludes --profiles
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,
    /// The profile set the invocation admits, comma-separated: the
    /// listed profiles whose match rules the entry's path satisfies,
    /// else the listed default, else none (an error); excludes
    /// --profile
    #[arg(long, value_name = "NAMES", value_parser = handlers::parse_profiles)]
    profiles: Option<handlers::ProfileSet>,
    /// A values file computed defaults resolve from: secrets and
    /// non-secret context variables alike, keyed by the computed
    /// default's one dependency. No other source is ever read
    #[arg(long, value_name = "FILE")]
    resolve_from: Option<PathBuf>,
    /// The most sources one reading invokes, entry documents and
    /// imports together (default 1024)
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    max_invocations: Option<u32>,
}

impl ProfileArgs {
    /// The data options these flags and the verb's locations and
    /// handlers make. `--profile` with `--profiles` is `KDLITH-CLI001`
    /// (exit 2), and a `--resolve-from` file loads here, before any
    /// definition.
    fn data_options(
        self,
        locations: LocationArgs,
        handlers: Vec<String>,
    ) -> Result<DataOptions, ExitStatus> {
        let set = match (self.profile.as_deref(), self.profiles) {
            (Some(_), Some(_)) => {
                return Err(usage::report_invalid_argument(
                    "the argument '--profile <NAME>' cannot be used with '--profiles <NAMES>'",
                ));
            }
            (_, Some(handlers::ProfileSet(names))) => names,
            (_, None) => Vec::new(),
        };
        let resolver = match &self.resolve_from {
            Some(path) => Some(resolve::load(path)?),
            None => None,
        };
        Ok(locations
            .apply(DataOptions::new(self.profile, handlers))
            .with_profiles(set)
            .with_resolver(resolver)
            .with_max_invocations(self.max_invocations))
    }
}

/// The `--scope` value of `vet` and `inspect data`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum ScopeArg {
    Local,
    Complete,
}

/// The `--scope` value of `read data`: the two reserved words, or a
/// scope name selecting only its own partitions.
#[derive(Clone, Debug)]
enum ReadScope {
    Local,
    Complete,
    Named(String),
}

/// The `--diagnostics` value of the data verbs.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum DiagnosticsArg {
    /// Every diagnostic of the run as one compact JSON line on stdout
    Json,
}

impl DiagnosticsArg {
    /// Whether the format arms the stdout JSON mirror.
    pub(crate) fn json(self) -> bool {
        matches!(self, DiagnosticsArg::Json)
    }
}

/// `kdlith inspect dialect|schema|data`.
#[derive(Args)]
struct InspectArgs {
    #[command(subcommand)]
    command: InspectCommand,
}

#[derive(Subcommand)]
enum InspectCommand {
    /// Show dialect definition documents and their imports as typed
    /// items, one JSON line each on stdout, then check the dialect as
    /// `vet dialect` does; any error exits nonzero after the items
    Dialect(InspectDefinitionArgs),
    /// Show schema definition documents and their imports as typed
    /// items, one JSON line each on stdout, then check the schema as
    /// `vet schema` does; any error exits nonzero after the items
    Schema(InspectSchemaArgs),
    /// Bind data documents exactly as `vet data` does: under the bundled
    /// dialect (or --dialect) and the --schema schema, checked first, or
    /// the dialect itself without --schema; each bound item goes to
    /// stdout as one JSON line
    Data(InspectDataArgs),
}

#[derive(Args)]
struct InspectDefinitionArgs {
    /// A dialect definition document replacing the bundled one
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// The item view to print: source (every item), effective (the
    /// compiled definition by section) or plan (its processing plan);
    /// bound, context, origins and references apply only to inspect
    /// data (KDLITH-CLI004, exit 2)
    #[arg(long, value_enum, default_value_t = ViewArg::Source)]
    view: ViewArg,
    /// Definition documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(Args)]
struct InspectSchemaArgs {
    /// A dialect definition document replacing the bundled one
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// The item view to print: source (every item), effective (the
    /// compiled definition by section) or plan (its processing plan);
    /// bound, context, origins and references apply only to inspect
    /// data (KDLITH-CLI004, exit 2)
    #[arg(long, value_enum, default_value_t = ViewArg::Source)]
    view: ViewArg,
    #[command(flatten)]
    locations: LocationArgs,
    /// Schema definition documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(Args)]
struct InspectDataArgs {
    /// A dialect definition document replacing the bundled one
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document; without one, data binds against
    /// the dialect itself
    #[arg(long = "schema", value_name = "FILE")]
    schema: Option<PathBuf>,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// The item view: source, bound (records, declarations,
    /// contributions, pending bindings), context, effective
    /// (declarations, resolutions), origins, plan or references (how
    /// each reference linked, by declared reference type)
    #[arg(long, value_enum, default_value_t = ViewArg::Source)]
    view: ViewArg,
    /// Bind the documents alone (local) or also link every reference a
    /// declared reference type looks up (complete); a link failure
    /// fails the run after the view has printed (references links
    /// under both, failing the run under complete only)
    #[arg(long, value_enum, default_value_t = ScopeArg::Complete)]
    scope: ScopeArg,
    /// Also write every diagnostic of the run to stdout as one compact
    /// JSON line per record, beside the unchanged stderr text
    #[arg(long = "diagnostics", value_enum, value_name = "FORMAT")]
    diagnostics: Option<DiagnosticsArg>,
    /// Data documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// The `--view` value of `inspect`: the item view to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ViewArg {
    Source,
    Bound,
    Effective,
    Origins,
    References,
    Context,
    Plan,
}

impl ViewArg {
    fn name(self) -> &'static str {
        match self {
            ViewArg::Source => "source",
            ViewArg::Bound => "bound",
            ViewArg::Effective => "effective",
            ViewArg::Origins => "origins",
            ViewArg::References => "references",
            ViewArg::Context => "context",
            ViewArg::Plan => "plan",
        }
    }
}

/// `kdlith sources plan|verify|list`.
#[derive(Args)]
struct SourcesArgs {
    #[command(subcommand)]
    command: SourcesCommand,
}

#[derive(Subcommand)]
enum SourcesCommand {
    /// Print the fetch plan, one JSON line per origin the --sources
    /// manifest declares: its origin, its pin or false, its path under
    /// the store, and whether the store holds a tree there
    Plan(SourcesVerbArgs),
    /// Re-hash every origin tree the store holds against its pin, one
    /// JSON line per origin; a mismatch or an absent tree fails the run
    Verify(SourcesVerbArgs),
    /// Print every source, one JSON line each: its kind, its pin, and the
    /// digest of the tree a reading reads, the value to pin it by
    List(SourcesVerbArgs),
}

#[derive(Args)]
struct SourcesVerbArgs {
    #[command(flatten)]
    locations: LocationArgs,
}

/// `kdlith list types|nodes|profiles|items`.
#[derive(Args)]
struct ListArgs {
    #[command(subcommand)]
    command: ListCommand,
}

#[derive(Subcommand)]
enum ListCommand {
    /// List the types the schema declares, one per line, sorted by name
    Types(ListSchemaArgs),
    /// List the node bindings the schema declares, one per line, sorted
    /// by name
    Nodes(ListSchemaArgs),
    /// List the profiles the schema declares, one per line, sorted by
    /// name
    Profiles(ListSchemaArgs),
    /// Index data documents through the schema and list each bound
    /// declaration and record by its identity, one per line, sorted
    Items(ListItemsArgs),
}

#[derive(Args)]
struct ListSchemaArgs {
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document, read under the bundled dialect or
    /// the one `--dialect` names
    #[arg(long, value_name = "FILE", required = true)]
    schema: PathBuf,
    #[command(flatten)]
    locations: LocationArgs,
}

#[derive(Args)]
struct ListItemsArgs {
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document, read under the bundled dialect or
    /// the one `--dialect` names
    #[arg(long, value_name = "FILE", required = true)]
    schema: PathBuf,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Data documents: the entrypoints
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// `kdlith cache inspect|refresh|clean --state <DIR>`. Every form names
/// its state directory explicitly; there is no default location.
#[derive(Args)]
struct CacheArgs {
    #[command(subcommand)]
    command: CacheCommand,
}

#[derive(Subcommand)]
enum CacheCommand {
    /// List each owned entry: cached results with their invocation
    /// identity and derivations, record snapshots with their entry,
    /// lineage, and counts; report entries not owned or not usable
    Inspect(CacheStateArgs),
    /// Read the data documents through the schema and replace the
    /// state's owned entries with the bound results and record context
    /// snapshots the reading derives
    Refresh(Box<CacheRefreshArgs>),
    /// Remove every owned entry, cached results and snapshots alike;
    /// report, never remove, anything else in the state directory
    Clean(CacheStateArgs),
}

#[derive(Args)]
struct CacheStateArgs {
    /// The state directory holding the cached results and snapshots
    #[arg(long, value_name = "DIR", required = true)]
    state: PathBuf,
}

#[derive(Args)]
struct CacheRefreshArgs {
    /// The state directory holding the cached results and snapshots;
    /// created when it does not exist, its parent must
    #[arg(long, value_name = "DIR", required = true)]
    state: PathBuf,
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document, read under the bundled dialect or
    /// the one `--dialect` names
    #[arg(long, value_name = "FILE", required = true)]
    schema: PathBuf,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Data documents: the entrypoints
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// `kdlith read [file <FILE>|data ...]`, with the bare one-argument
/// form `kdlith read <FILE>` accepted as a compatibility spelling of
/// `read file`.
#[derive(Args)]
struct ReadArgs {
    /// A KDL document, in the bare compatibility form `kdlith read
    /// <FILE>`; a document named `file` or `data` needs the explicit
    /// `read file` form
    file: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<ReadCommand>,
}

#[derive(Subcommand)]
enum ReadCommand {
    /// Parse one document to its clean end
    File(ReadFileArgs),
    /// Read one indexed data record by its identity through a schema
    Data(Box<ReadDataArgs>),
}

#[derive(Args)]
struct ReadFileArgs {
    /// A KDL document
    file: PathBuf,
}

#[derive(Args)]
struct ReadDataArgs {
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document, read under the bundled dialect or
    /// the one `--dialect` names
    #[arg(long, required = true)]
    schema: PathBuf,
    /// An entry document bound before the read document as its own
    /// invocation of the same session; its declarations are visible to
    /// the read document, its context is its own
    #[arg(long)]
    entry: Option<PathBuf>,
    /// A state directory a cache refresh wrote: the record restores the
    /// context it bound under and, of the read document, only its span
    /// binds. A miss replays; another schema or entry chain is CTX004
    #[arg(long, value_name = "DIR")]
    state: Option<PathBuf>,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Look the identity up over every (target, scope) partition the
    /// resolver settled (complete), only the scope-less partition
    /// (local), or only the partitions of the kebab-form scope NAME; a
    /// record matches under each
    #[arg(
        long,
        value_name = "SCOPE",
        default_value = "complete",
        value_parser = handlers::parse_read_scope
    )]
    scope: ReadScope,
    /// Also write every diagnostic of the run to stdout as one compact
    /// JSON line per record, beside the unchanged stderr text
    #[arg(long = "diagnostics", value_enum, value_name = "FORMAT")]
    diagnostics: Option<DiagnosticsArg>,
    /// The data document holding the record
    file: PathBuf,
    /// The identity of the record to read
    id: String,
}

/// The `--delivery` value of `export`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum DeliveryArg {
    /// Stage output until processing succeeds, then publish in one step
    Atomic,
    /// Emit completed items immediately; a failing exit invalidates the
    /// stream
    Provisional,
}

/// The explicit output policy every sensitive field passes.
#[derive(Clone, Copy, ValueEnum)]
enum SensitiveArg {
    /// Write sensitive values as they are
    Emit,
    /// Leave sensitive fields out of the output
    Omit,
    /// Replace sensitive values with a redaction marker
    Redact,
}

/// `kdlith export --schema <FILE> <files...>`.
#[derive(Args)]
struct ExportArgs {
    /// A dialect definition document replacing the bundled one; it is
    /// itself vetted with complete checking
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// A schema definition document, read under the bundled dialect or
    /// the one `--dialect` names
    #[arg(long, value_name = "FILE", required = true)]
    schema: PathBuf,
    /// What to emit: `roots` for the entry documents' top-level nodes,
    /// or the name of a record type
    #[arg(long, value_name = "TYPE|roots", required = true)]
    emit: String,
    /// The output format; only ndjson exists
    #[arg(
        long,
        value_name = "FORMAT",
        required = true,
        value_parser = parse_export_format
    )]
    format: String,
    /// The file results are written to; stdout when absent
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// When results become visible
    #[arg(long, value_enum, default_value_t = DeliveryArg::Atomic)]
    delivery: DeliveryArg,
    /// The output policy every sensitive field passes; when absent a
    /// secret is redacted and a reference emitted
    #[arg(long, value_enum, value_name = "POLICY")]
    sensitive: Option<SensitiveArg>,
    /// Bind the documents alone (local) or also link every reference a
    /// declared reference type looks up, failing the run when one does
    /// not resolve to exactly one declaration (complete)
    #[arg(long, value_enum, default_value_t = ScopeArg::Complete)]
    scope: ScopeArg,
    /// Also write every diagnostic of the run to stdout as one compact
    /// JSON line per record, beside the unchanged stderr text
    #[arg(long = "diagnostics", value_enum, value_name = "FORMAT")]
    diagnostics: Option<DiagnosticsArg>,
    #[command(flatten)]
    locations: LocationArgs,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Data documents: the entrypoints
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// Parse the process arguments, dispatch the verb, and resolve the
/// exit stratum. Argument errors print their registered code
/// (`KDLITH-CLI001`-`004`) with clap's statement of the fault and exit
/// 2; `--help` and `--version` print to stdout and exit 0.
pub fn run() -> ExitStatus {
    run_with_sources(std::env::args_os(), Vec::new())
}

/// Run the command line `args` (the program name first) as [`run`]
/// does, with `sources` registered after the command line's own on
/// every verb that reads a schema or data: a host binary serves the
/// verbs over named sources it holds, such as files it embeds. A name
/// registered twice is `KDLITH-LOC005`.
pub fn run_with_sources<I, T>(args: I, sources: Vec<(String, SourceTree)>) -> ExitStatus
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => return usage::report_parse_error(&error),
    };
    let mut strict = false;
    if let Some(locations) = cli.command.locations() {
        locations.hosted = sources;
        strict = locations.warnings_as_errors;
    }
    report::fail_on_warnings(strict);
    match cli.command {
        Command::Gen(args) => generate::run(args.command),
        Command::Vet(args) => vet::run(args.command),
        Command::Export(args) => export::run(args),
        Command::Inspect(args) => inspect::run(args.command),
        Command::List(args) => list::run(args.command),
        Command::Read(args) => read::run(&args),
        Command::Cache(args) => cache::run(args.command),
        Command::Sources(args) => sources::run(args.command),
        Command::Changelog => changelog::run(),
        Command::Help(args) => help(&args.command),
    }
}

impl Command {
    /// The location arguments of a verb that reads a schema or data.
    fn locations(&mut self) -> Option<&mut LocationArgs> {
        Some(match self {
            Command::Gen(args) => match &mut args.command {
                GenCommand::Schema(args) => &mut args.locations,
                GenCommand::Workspace(args) => &mut args.locations,
                GenCommand::Dialect(_) => return None,
            },
            Command::Vet(args) => match &mut args.command {
                VetCommand::Schema(args) => &mut args.locations,
                VetCommand::Data(args) => &mut args.locations,
                VetCommand::Dialect(_) => return None,
            },
            Command::Export(args) => &mut args.locations,
            Command::Inspect(args) => match &mut args.command {
                InspectCommand::Schema(args) => &mut args.locations,
                InspectCommand::Data(args) => &mut args.locations,
                InspectCommand::Dialect(_) => return None,
            },
            Command::List(args) => match &mut args.command {
                ListCommand::Types(args)
                | ListCommand::Nodes(args)
                | ListCommand::Profiles(args) => &mut args.locations,
                ListCommand::Items(args) => &mut args.locations,
            },
            Command::Read(args) => match &mut args.command {
                Some(ReadCommand::Data(args)) => &mut args.locations,
                Some(ReadCommand::File(_)) | None => return None,
            },
            Command::Cache(args) => match &mut args.command {
                CacheCommand::Refresh(args) => &mut args.locations,
                CacheCommand::Inspect(_) | CacheCommand::Clean(_) => return None,
            },
            Command::Sources(args) => match &mut args.command {
                SourcesCommand::Plan(args)
                | SourcesCommand::Verify(args)
                | SourcesCommand::List(args) => &mut args.locations,
            },
            Command::Changelog | Command::Help(_) => return None,
        })
    }
}

/// Print the help of the named command path, or the root help when no
/// command is named. The usage line carries the full verb path.
fn help(path: &[String]) -> ExitStatus {
    let mut root = Cli::command();
    let mut current = &mut root;
    let mut full = String::from("kdlith");
    for name in path {
        full.push(' ');
        full.push_str(name);
        match current.find_subcommand_mut(name) {
            Some(sub) => current = sub,
            None => {
                return usage::report_invalid_argument(&format!("unrecognized command '{name}'"));
            }
        }
    }
    let mut shown = current.clone().bin_name(full);
    let _ = shown.print_help();
    println!();
    ExitStatus::Success
}

/// `export --format` is validated up front and only `ndjson` is
/// supported; anything else is a KDLITH-CLI003 rejection, never a
/// silently ignored value.
fn parse_export_format(value: &str) -> Result<String, usage::UnknownFormat> {
    if value == "ndjson" {
        Ok(value.to_owned())
    } else {
        Err(usage::UnknownFormat::new(value, "ndjson"))
    }
}
