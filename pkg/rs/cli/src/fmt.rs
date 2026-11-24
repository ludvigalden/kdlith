//! `kdlith fmt file|schema|data|auto`: write documents in the canonical
//! layout, in place, to stdout, or only check them. `schema` and `data`
//! take the layout rules a schema declares; `file` needs no schema;
//! `auto` reads each document in the mode it calls for, as a tool that
//! knows which schema governs which file (relmap, from a map) asks.

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use kdlith_diagnostic::{DiagnosticRecord, ExitStatus};
use kdlith_format::{FormatError, Limits, Options, Policy};

use crate::read::source_error;
use crate::report::report_all;

/// What to do with each formatted document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Replace the file when its text differs.
    Write,
    /// Report every file whose text differs; write nothing.
    Check,
    /// Print the formatted text; write nothing.
    Stdout,
}

/// The settings every `fmt` form shares.
pub(crate) struct Run {
    pub(crate) mode: Mode,
    pub(crate) width: Option<usize>,
    pub(crate) content_width: Option<usize>,
}

impl Run {
    fn options(&self) -> Options {
        let base = Limits::default();
        Options::default().with_limits(Limits {
            width: self.width.unwrap_or(base.width),
            content_width: self.content_width.unwrap_or(base.content_width),
        })
    }
}

/// Run a parsed `fmt` subcommand.
pub(crate) fn dispatch(command: crate::FmtCommand) -> ExitStatus {
    match command {
        crate::FmtCommand::File(args) => run(&args.files, &args.output.run(), None),
        crate::FmtCommand::Schema(args) => {
            let policy = match crate::layout::meta_schema_policy(&args) {
                Ok(policy) => policy,
                Err(status) => return status,
            };
            run(&args.files, &args.output.run(), Some(policy))
        }
        crate::FmtCommand::Data(args) => {
            let policy = match crate::layout::schema_policy(&args) {
                Ok(policy) => policy,
                Err(status) => return status,
            };
            run(&args.files, &args.output.run(), Some(policy))
        }
        crate::FmtCommand::Auto(args) => auto(&args),
    }
}

/// The prefix that names the schema of the documents after it.
const SCHEMA_ITEM: &str = "--schema=";

/// `fmt auto`: each document under the schema the nearest `--schema=FILE`
/// before it names; under none, a schema definition (its first node is
/// `schema`) under the meta-schema, and any other document by the layout
/// alone. Each schema is compiled once.
fn auto(args: &crate::FmtAutoArgs) -> ExitStatus {
    let run = args.output.run();
    let mut named: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut schema: Option<PathBuf> = None;
    for item in &args.items {
        let text = item.to_string_lossy();
        if let Some(rest) = text.strip_prefix(SCHEMA_ITEM) {
            schema = (!rest.is_empty()).then(|| PathBuf::from(rest));
        } else if text.starts_with('-') && text != "-" {
            let record = source_error("KDLITH-FMT004", &[&text]);
            return report_all(&[record]);
        } else {
            named.push((PathBuf::from(item), schema.clone()));
        }
    }
    let mut files: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for (path, schema) in named {
        match expand(std::slice::from_ref(&path)) {
            Ok(found) => files.extend(found.into_iter().map(|file| (file, schema.clone()))),
            Err(record) => return report_all(&[*record]),
        }
    }
    let mut policies: BTreeMap<Option<PathBuf>, Option<Policy>> = BTreeMap::new();
    let mut failed = ExitStatus::Success;
    let mut sources: Vec<Option<String>> = Vec::with_capacity(files.len());
    for (file, schema) in &mut files {
        // A document under no schema is read once to see whether it is a
        // schema definition; its text is kept for formatting.
        let source = if schema.is_none() {
            read(file).ok()
        } else {
            None
        };
        let key = match (&schema, &source) {
            (Some(schema), _) => Some(schema.clone()),
            (None, Some(source)) if is_schema_definition(source) => Some(PathBuf::from("-")),
            (None, _) => None,
        };
        if let Some(key) = &key
            && !policies.contains_key(&Some(key.clone()))
        {
            let compiled = if key.as_os_str() == "-" {
                crate::layout::auto_meta_schema_policy(args)
            } else {
                crate::layout::auto_schema_policy(args, key)
            };
            let policy = match compiled {
                Ok(mut policy) => {
                    policy.override_limits(run.width, run.content_width);
                    Some(policy)
                }
                Err(status) => {
                    failed = status;
                    None
                }
            };
            policies.insert(Some(key.clone()), policy);
        }
        *schema = key;
        sources.push(source);
    }
    let options = run.options();
    let jobs: Vec<Job<'_>> = files
        .iter()
        .zip(sources)
        .filter_map(|((file, key), source)| {
            let policy = match key {
                // A schema that failed to compile was reported; its
                // documents are left alone.
                Some(key) => Some(policies.get(&Some(key.clone()))?.as_ref()?),
                None => None,
            };
            Some(Job {
                file,
                policy,
                source,
            })
        })
        .collect();
    let status = settle(&jobs, &run, &options);
    if failed == ExitStatus::Success {
        status
    } else {
        failed
    }
}

/// Whether a document's first node is `schema`: a schema definition.
fn is_schema_definition(source: &str) -> bool {
    kdlith_parse::tree::Document::parse(source).is_ok_and(|document| {
        document
            .block
            .items
            .iter()
            .find_map(|item| match &item.kind {
                kdlith_parse::tree::ItemKind::Node(node) if !node.slashdash => {
                    Some(node.name.as_str() == Some("schema"))
                }
                _ => None,
            })
            == Some(true)
    })
}

/// One document's outcome.
enum Outcome {
    /// The formatted text, and whether it differs from the document.
    Formatted(String, bool),
    Failed(DiagnosticRecord),
}

/// One document to format: its path, the policy it is read under, and
/// its text where it was already read.
struct Job<'a> {
    file: &'a Path,
    policy: Option<&'a Policy>,
    source: Option<String>,
}

/// Format every named document (a directory names every `.kdl` file
/// under it, `-` standard input) under `policy`; command-line limits
/// override the policy's.
pub(crate) fn run(paths: &[PathBuf], run: &Run, policy: Option<Policy>) -> ExitStatus {
    let mut policy = policy;
    if let Some(policy) = &mut policy {
        policy.override_limits(run.width, run.content_width);
    }
    let files = match expand(paths) {
        Ok(files) => files,
        Err(record) => return report_all(&[*record]),
    };
    let jobs: Vec<Job<'_>> = files
        .iter()
        .map(|file| Job {
            file,
            policy: policy.as_ref(),
            source: None,
        })
        .collect();
    settle(&jobs, run, &run.options())
}

/// Format the jobs and act on each outcome as `run.mode` says.
fn settle(jobs: &[Job<'_>], run: &Run, options: &Options) -> ExitStatus {
    let outcomes = format_all(jobs, options);
    let mut records = Vec::new();
    let mut stdout = std::io::stdout().lock();
    for (job, outcome) in jobs.iter().zip(outcomes) {
        let file = job.file;
        let shown = file.display().to_string();
        match outcome {
            Outcome::Failed(record) => records.push(record),
            Outcome::Formatted(text, _) if run.mode == Mode::Stdout || is_stdin(file) => {
                let _ = stdout.write_all(text.as_bytes());
            }
            Outcome::Formatted(_, false) => {}
            Outcome::Formatted(text, true) => match run.mode {
                Mode::Check => {
                    records.push(fmt_error("KDLITH-FMT001", &[&shown]).with_source(shown));
                }
                Mode::Write | Mode::Stdout => {
                    if let Err(error) = replace(file, &text) {
                        let record = fmt_error("KDLITH-FMT003", &[&shown, &error.to_string()]);
                        records.push(record.with_source(shown));
                    }
                }
            },
        }
    }
    let _ = stdout.flush();
    report_all(&records)
}

fn is_stdin(path: &Path) -> bool {
    path.as_os_str() == "-"
}

fn fmt_error(code: &str, values: &[&str]) -> DiagnosticRecord {
    source_error(code, values)
}

/// The documents `paths` name, in the order named; a directory's `.kdl`
/// files in path order, its hidden entries and links passed over.
fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<DiagnosticRecord>> {
    let mut files = Vec::new();
    for path in paths {
        if !is_stdin(path) && path.is_dir() {
            let mut found = Vec::new();
            walk(path, &mut found).map_err(|error| {
                let shown = path.display().to_string();
                Box::new(
                    source_error("KDLITH-SRC001", &[&shown, &error.to_string()]).with_source(shown),
                )
            })?;
            found.sort();
            files.extend(found);
        } else {
            files.push(path.clone());
        }
    }
    Ok(files)
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, found)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "kdl") {
            found.push(path);
        }
    }
    Ok(())
}

/// Format the jobs on as many threads as the machine offers, each
/// outcome in its job's place.
fn format_all(jobs: &[Job<'_>], options: &Options) -> Vec<Outcome> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(jobs.len().max(1));
    if threads <= 1 {
        return jobs.iter().map(|job| one(job, options)).collect();
    }
    let chunk = jobs.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || part.iter().map(|job| one(job, options)).collect::<Vec<_>>())
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

fn read(path: &Path) -> Result<String, Box<DiagnosticRecord>> {
    let shown = path.display().to_string();
    let bytes = if is_stdin(path) {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).map(|_| bytes)
    } else {
        std::fs::read(path)
    }
    .map_err(|error| Box::new(source_error("KDLITH-SRC001", &[&shown, &error.to_string()])))?;
    String::from_utf8(bytes).map_err(|error| {
        let at = error.utf8_error().valid_up_to().to_string();
        Box::new(source_error("KDLITH-SRC002", &[&shown, &at]))
    })
}

fn one(job: &Job<'_>, options: &Options) -> Outcome {
    let shown = job.file.display().to_string();
    let read_now;
    let source = match &job.source {
        Some(source) => source,
        None => match read(job.file) {
            Ok(source) => {
                read_now = source;
                &read_now
            }
            Err(record) => return Outcome::Failed(record.with_source(shown)),
        },
    };
    match kdlith_format::format(source, options, job.policy) {
        Ok(text) => {
            let changed = text != *source;
            Outcome::Formatted(text, changed)
        }
        Err(FormatError::Parse(error)) => {
            Outcome::Failed(error.to_diagnostic(source).with_source(shown))
        }
        Err(_) => Outcome::Failed(fmt_error("KDLITH-FMT002", &[&shown]).with_source(shown)),
    }
}

/// Replace `path` with `text` in one step: written beside it, given its
/// permissions, then renamed over it.
fn replace(path: &Path, text: &str) -> std::io::Result<()> {
    let permissions = std::fs::metadata(path)?.permissions();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staged = path.with_file_name(format!(".{name}.kdlith-fmt"));
    let written = std::fs::File::create(&staged).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::set_permissions(&staged, permissions)
    });
    match written.and_then(|()| std::fs::rename(&staged, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&staged);
            Err(error)
        }
    }
}
