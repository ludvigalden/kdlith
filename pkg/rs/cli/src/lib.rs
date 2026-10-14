//! Commands, arguments, output conventions, and orchestration: a
//! verb-first clap command tree (gen, vet, export, inspect, list,
//! read, cache, changelog, plus help) behind three global options.
mod export;
mod generate;
mod handlers;
mod inspect;
mod read;
mod report;
mod vet;

use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use kdlith_diagnostic::ExitStatus;

pub use export::export;
pub use generate::{GenMode, GenTarget, generate};
pub use inspect::inspect;
pub use read::read;
pub use vet::{DataOptions, VetTarget, vet, vet_data};

/// The whole command line: global options plus one verb.
#[derive(Parser)]
#[command(
    name = "kdlith",
    bin_name = "kdlith",
    about = "Read typed KDL, validate definitions, and generate Rust processors",
    version,
    disable_help_subcommand = true
)]
struct Cli {
    /// Stderr verbosity band, 0–10
    #[arg(
        short = 'v',
        long,
        value_name = "N",
        global = true,
        default_value_t = 2,
        value_parser = parse_verbosity
    )]
    verbosity: u8,
    /// Equivalent to --verbosity 10
    #[arg(long, global = true)]
    debug: bool,
    /// Command response format
    #[arg(
        long,
        value_name = "FORMAT",
        global = true,
        default_value = "text",
        value_parser = parse_stdout_format
    )]
    stdout_format: String,
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
    List,
    /// Read one definition or indexed data item
    Read(ReadArgs),
    /// Inspect, refresh, or clean derived processing state
    Cache,
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
    Schema(GenFormArgs),
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
    /// writing; report drift and exit nonzero on any difference
    #[arg(long, conflicts_with = "dry_run")]
    check: bool,
    /// Definition documents: the dialect entrypoint, or the schema's
    /// documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
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
    /// Bind data documents against the schema `--schema` names, under
    /// the bundled dialect or the one `--dialect` names, or against
    /// the dialect itself without `--schema`; bound items go to stdout
    /// as one JSON line each
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
    /// A discovery anchor the data path expands import expressions
    /// against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
    /// The profile the invocation binds
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// Data documents
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// The `--scope` value of `vet dialect` and `vet schema`.
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
    /// Read dialect definition documents and their imports through the
    /// reference processor; each typed item goes to stdout as one JSON
    /// line
    Dialect(InspectDefinitionArgs),
    /// Read schema definition documents and their imports through the
    /// reference processor; each typed item goes to stdout as one JSON
    /// line
    Schema(InspectDefinitionArgs),
    /// Bind data documents under the bundled dialect (or the one given
    /// by --dialect) and the schema given by --schema, or against the
    /// dialect itself without --schema; each bound item goes to stdout
    /// as one JSON line
    Data(InspectDataArgs),
}

#[derive(Args)]
struct InspectDefinitionArgs {
    /// A dialect definition document replacing the bundled one
    #[arg(long = "dialect", value_name = "FILE")]
    dialect: Option<PathBuf>,
    /// The item view to print; only the source view exists for now
    #[arg(long, value_enum, default_value_t = ViewArg::Source)]
    view: ViewArg,
    /// Definition documents
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
    /// A discovery anchor the data path expands import expressions
    /// against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
    /// The profile the invocation binds
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,
    /// A built-in context handler to install, by name (repeatable)
    #[arg(long = "handler", value_name = "NAME", value_parser = handlers::parse_handler)]
    handlers: Vec<String>,
    /// The item view to print; only the source view exists for now
    #[arg(long, value_enum, default_value_t = ViewArg::Source)]
    view: ViewArg,
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
    Data(ReadDataArgs),
}

#[derive(Args)]
struct ReadFileArgs {
    /// A KDL document
    file: PathBuf,
}

#[derive(Args)]
struct ReadDataArgs {
    /// A schema definition document, read under the bundled dialect
    #[arg(long, required = true)]
    schema: PathBuf,
    /// An entry document bound before the read document, establishing
    /// its declarations and context
    #[arg(long)]
    entry: Option<PathBuf>,
    /// A discovery anchor the data path expands import expressions
    /// against, as NAME=PATH (repeatable)
    #[arg(
        long = "anchor",
        value_name = "NAME=PATH",
        value_parser = handlers::parse_anchor
    )]
    anchors: Vec<(String, PathBuf)>,
    /// The profile the invocation binds
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,
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

/// `kdlith export --schema <FILE> <files...>`.
#[derive(Args)]
struct ExportArgs {
    /// A schema definition document, read under the bundled dialect
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
    /// Data documents: the entrypoints
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// Parse the process arguments, dispatch the verb, and resolve the
/// exit stratum. Usage errors print clap's diagnostic and exit 2;
/// `--help` and `--version` print to stdout and exit 0.
pub fn run() -> ExitStatus {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return match error.exit_code() {
                0 => ExitStatus::Success,
                2 => ExitStatus::Usage,
                _ => ExitStatus::Failure,
            };
        }
    };
    // The verbosity band is accepted and validated everywhere; --debug
    // is its top end. Per-verb output wiring consumes it later.
    let _band = if cli.debug { 10 } else { cli.verbosity };
    let _format = cli.stdout_format.as_str();
    match cli.command {
        Command::Gen(args) => generate::run(args.command),
        Command::Vet(args) => vet::run(args.command),
        Command::Export(args) => export::run(args),
        Command::Inspect(args) => inspect::run(args.command),
        Command::List => stub("list"),
        Command::Read(args) => read::run(&args),
        Command::Cache => stub("cache"),
        Command::Changelog => stub("changelog"),
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
                eprintln!("kdlith: unrecognized command {name}");
                return ExitStatus::Usage;
            }
        }
    }
    let mut shown = current.clone().bin_name(full);
    let _ = shown.print_help();
    println!();
    ExitStatus::Success
}

/// Every not-yet-wired verb fails closed with a clear diagnostic
/// rather than pretending to work.
fn stub(verb: &str) -> ExitStatus {
    eprintln!("kdlith: {verb} is not implemented yet");
    ExitStatus::Failure
}

/// `--verbosity` takes a number from 0 to 10.
fn parse_verbosity(value: &str) -> Result<u8, String> {
    let band = value
        .parse::<u8>()
        .map_err(|_| "verbosity takes a number from 0 to 10".to_owned())?;
    if band <= 10 {
        Ok(band)
    } else {
        Err("verbosity takes a number from 0 to 10".to_owned())
    }
}

/// `--stdout-format` is validated up front and only `text` is
/// supported for now; anything else is a clear rejection, never a
/// silently ignored value.
fn parse_stdout_format(value: &str) -> Result<String, String> {
    if value == "text" {
        Ok(value.to_owned())
    } else {
        Err(format!(
            "only the text stdout format is supported for now, not {value}"
        ))
    }
}

/// `export --format` is validated up front and only `ndjson` is
/// supported; anything else is a clear rejection, never a silently
/// ignored value.
fn parse_export_format(value: &str) -> Result<String, String> {
    if value == "ndjson" {
        Ok(value.to_owned())
    } else {
        Err(format!(
            "only the ndjson export format exists for now, not {value}"
        ))
    }
}
