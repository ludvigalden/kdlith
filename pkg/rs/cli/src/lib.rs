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
mod usage;
mod vet;

use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use kdlith_diagnostic::ExitStatus;

pub use export::export;
pub use generate::{GenMode, GenTarget, generate};
pub use inspect::inspect;
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

/// `kdlith gen dialect|schema`.
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
    Schema(GenSchemaArgs),
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
}

#[derive(Args)]
struct GenSchemaArgs {
    #[command(flatten)]
    form: GenFormArgs,
    /// A discovery anchor the schema documents' own import expressions
    /// expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
    /// A built-in context handler the schema may name, by name
    /// (repeatable); generation admits the name and never runs it
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// The directory the generated manifest roots its kdlith path
    /// dependencies at, written verbatim (default: the sibling layout)
    #[arg(
        long = "kdlith-path",
        value_name = "PATH",
        conflicts_with = "kdlith_version",
        value_parser = handlers::parse_kdlith_path
    )]
    kdlith_path: Option<String>,
    /// The version the generated manifest requests the kdlith crates at
    /// from the registry, written verbatim
    #[arg(
        long = "kdlith-version",
        value_name = "VER",
        value_parser = handlers::parse_kdlith_version
    )]
    kdlith_version: Option<String>,
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
    /// A discovery anchor the schema documents' own import expressions
    /// expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// Data documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
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
    /// else the listed default, else the first listed; excludes
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
    /// An import alias, as NAME=DIR (repeatable): an import written
    /// $NAME/PATH resolves PATH inside DIR wherever the importing
    /// document lives, confined to DIR instead of the entry's directory
    #[arg(long = "alias", value_name = "NAME=DIR", value_parser = handlers::parse_alias)]
    aliases: Vec<(String, PathBuf)>,
}

impl ProfileArgs {
    /// The data options these flags and the verb's anchors and handlers
    /// make. `--profile` with `--profiles` is `KDLITH-CLI001` (exit 2),
    /// and a `--resolve-from` file loads here, before any definition.
    fn data_options(
        self,
        anchors: Vec<(String, PathBuf)>,
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
        Ok(DataOptions::new(anchors, self.profile, handlers)
            .with_profiles(set)
            .with_resolver(resolver)
            .with_max_invocations(self.max_invocations)
            .with_aliases(self.aliases))
    }
}

/// The `--scope` value of `vet` and `inspect data`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum ScopeArg {
    Local,
    Complete,
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
    /// A discovery anchor the schema documents' own import expressions
    /// expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// A discovery anchor the schema documents' own import expressions
    /// expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
/// form `kdlith read <FILE>` accepted as a compatibility alias of
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
    #[command(flatten)]
    profiles: ProfileArgs,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
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
    /// A discovery anchor the schema's and the data documents' import
    /// expressions expand against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return usage::report_parse_error(&error),
    };
    match cli.command {
        Command::Gen(args) => generate::run(args.command),
        Command::Vet(args) => vet::run(args.command),
        Command::Export(args) => export::run(args),
        Command::Inspect(args) => inspect::run(args.command),
        Command::List(args) => list::run(args.command),
        Command::Read(args) => read::run(&args),
        Command::Cache(args) => cache::run(args.command),
        Command::Changelog => changelog::run(),
        Command::Help(args) => help(&args.command),
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
