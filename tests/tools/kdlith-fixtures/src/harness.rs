//! The harness run: option validation, fail-closed suite discovery,
//! scenario selection, trials over libtest-mimic, scratch retention,
//! and the exit classes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libtest_mimic::{Arguments, Failed, Trial};

use crate::fxcfg::{self, DEFAULT_SCRATCH_ROOT, FxOptions};
use crate::outcome::{self, RunEnv};
use crate::scratch::{self, RunDir};
use crate::suite::{self, Entry};
use crate::walk;

/// `0`: every selected scenario passed.
pub(crate) const EXIT_OK: u8 = 0;
/// `1`: a scenario failed, or the suite is missing, unreadable, empty,
/// or filtered to nothing.
pub(crate) const EXIT_FAIL: u8 = 1;
/// `2`: a usage error.
pub(crate) const EXIT_USAGE: u8 = 2;

const USAGE: &str = "usage: kdlith-fixtures --fx-suite <dir> --fx-kdlith <path> \
[--fx-scratch-root <dir>] [--fx-filter <substr>] [--fx-list] [--fx-keep-failed] \
[libtest-mimic options]";

/// Run the harness CLI over argv and return the process exit code.
pub(crate) fn run(argv: &[String]) -> u8 {
    let (opts, rest) = match fxcfg::parse(argv) {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("kdlith-fixtures: {msg}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    let (Some(suite_arg), Some(kdlith_arg)) = (opts.suite.as_deref(), opts.kdlith.as_deref())
    else {
        eprintln!("kdlith-fixtures: --fx-suite and --fx-kdlith are required\n{USAGE}");
        return EXIT_USAGE;
    };
    let Some(suite_root) = resolve(suite_arg, "suite root", Path::is_dir) else {
        return EXIT_FAIL;
    };
    let Some(kdlith) = resolve(kdlith_arg, "kdlith binary", Path::is_file) else {
        return EXIT_FAIL;
    };

    let selected = match select(&suite_root, &opts) {
        Ok(entries) => entries,
        Err(msg) => {
            eprintln!("kdlith-fixtures: FAIL: {msg}");
            return EXIT_FAIL;
        }
    };

    if opts.list {
        for entry in &selected {
            println!("{}", entry.name);
        }
        println!("{} scenario(s)", selected.len());
        return EXIT_OK;
    }

    run_trials(&opts, rest, &suite_root, &kdlith, selected)
}

/// Canonicalize a required path argument and check its kind, reporting
/// a loud failure when either step fails.
fn resolve(path: &Path, what: &str, kind_ok: fn(&Path) -> bool) -> Option<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(p) if kind_ok(&p) => Some(p),
        Ok(p) => {
            eprintln!(
                "kdlith-fixtures: FAIL: {what} {} has the wrong kind",
                p.display()
            );
            None
        }
        Err(e) => {
            eprintln!("kdlith-fixtures: FAIL: {what} {}: {e}", path.display());
            None
        }
    }
}

/// Discover the suite and apply `--fx-filter`. A suite with zero
/// scenarios, or a filter that selects none, is an error: an empty
/// selection never passes.
fn select(suite_root: &Path, opts: &FxOptions) -> Result<Vec<Entry>, String> {
    let found = suite::discover(suite_root)?;
    for name in &found.skipped {
        eprintln!(
            "kdlith-fixtures: note: {} is not a scenario (no scenario.json); not run",
            suite_root.join(name).display()
        );
    }
    if found.entries.is_empty() {
        return Err(format!(
            "suite {} contains zero scenarios; an empty suite never passes",
            suite_root.display()
        ));
    }
    let total = found.entries.len();
    let selected: Vec<Entry> = match &opts.filter {
        None => found.entries,
        Some(f) => found
            .entries
            .into_iter()
            .filter(|e| e.name.contains(f.as_str()))
            .collect(),
    };
    if selected.is_empty() {
        return Err(format!(
            "--fx-filter {:?} selects zero of {total} scenario(s); an empty selection never passes",
            opts.filter.as_deref().unwrap_or_default()
        ));
    }
    Ok(selected)
}

/// Build one trial per selected scenario, run them, and clean up.
fn run_trials(
    opts: &FxOptions,
    rest: Vec<String>,
    suite_root: &Path,
    kdlith: &Path,
    selected: Vec<Entry>,
) -> u8 {
    let scratch_root = opts
        .scratch_root
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SCRATCH_ROOT));
    let run_dir = match RunDir::create(&scratch_root) {
        Ok(r) => Arc::new(r),
        Err(msg) => {
            eprintln!("kdlith-fixtures: FAIL: {msg}");
            return EXIT_FAIL;
        }
    };
    // One descriptor for the whole run: every scratch classification
    // below walks from here, never following a link.
    let root_fd = match walk::open_root(&run_dir.root) {
        Ok(fd) => Arc::new(fd),
        Err(e) => {
            eprintln!(
                "kdlith-fixtures: FAIL: cannot open run root {}: {e}",
                run_dir.root.display()
            );
            run_dir.finish(&[]);
            return EXIT_FAIL;
        }
    };
    let harness = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("kdlith-fixtures: FAIL: cannot locate the harness binary: {e}");
            run_dir.finish(&[]);
            return EXIT_FAIL;
        }
    };
    if let Err(msg) = crate::fxbin::populate_bin(&run_dir.bin_dir(), &harness) {
        eprintln!("kdlith-fixtures: FAIL: {msg}");
        run_dir.finish(&[]);
        return EXIT_FAIL;
    }

    let kept: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let suite_root: Arc<Path> = Arc::from(suite_root);
    let kdlith: Arc<Path> = Arc::from(kdlith);
    let trials: Vec<Trial> = selected
        .into_iter()
        .map(|entry| {
            let ctx = TrialCtx {
                entry: entry.clone(),
                run_dir: Arc::clone(&run_dir),
                root_fd: Arc::clone(&root_fd),
                suite_root: Arc::clone(&suite_root),
                kdlith: Arc::clone(&kdlith),
                keep_failed: opts.keep_failed,
                kept: Arc::clone(&kept),
            };
            Trial::test(entry.name, move || ctx.run())
        })
        .collect();

    let args = Arguments::from_iter(rest);
    let conclusion = libtest_mimic::run(&args, trials);

    let kept = kept.lock().map(|k| k.clone()).unwrap_or_default();
    run_dir.finish(&kept);
    for dir in &kept {
        eprintln!("kdlith-fixtures: kept failed scenario at {}", dir.display());
    }
    if args.list {
        return EXIT_OK;
    }
    if conclusion.num_passed + conclusion.num_failed == 0 {
        eprintln!(
            "kdlith-fixtures: FAIL: zero scenarios ran ({} filtered out, {} ignored); an empty run never passes",
            conclusion.num_filtered_out, conclusion.num_ignored
        );
        return EXIT_FAIL;
    }
    if conclusion.has_failed() {
        EXIT_FAIL
    } else {
        EXIT_OK
    }
}

/// Everything one trial needs, owned so the trial can run on any thread.
struct TrialCtx {
    entry: Entry,
    run_dir: Arc<RunDir>,
    root_fd: Arc<rustix::fd::OwnedFd>,
    suite_root: Arc<Path>,
    kdlith: Arc<Path>,
    keep_failed: bool,
    kept: Arc<Mutex<Vec<PathBuf>>>,
}

impl TrialCtx {
    fn run(&self) -> Result<(), Failed> {
        // A manifest that fails to load is a failed trial, never a skip.
        let scenario = crate::scenario::load(&self.entry.dir)
            .map_err(|msg| Failed::from(format!("scenario load error: {msg}")))?;
        let cwd = self
            .run_dir
            .materialize(&self.entry.name, &self.entry.dir)
            .map_err(|msg| Failed::from(format!("materialization error: {msg}")))?;
        let env = RunEnv {
            kdlith: &self.kdlith,
            suite: &self.suite_root,
            scratch: &self.run_dir.root,
            bin_dir: &self.run_dir.bin_dir(),
            root_fd: &self.root_fd,
        };
        let result = match &scenario {
            crate::scenario::Scenario::Invoke(invocation) => {
                outcome::execute(invocation, &cwd, &env, &self.entry.dir)
            }
            crate::scenario::Scenario::Oracle(oracle) => crate::oracle::run(oracle, &cwd, &env),
        };
        match result {
            Ok(()) => {
                scratch::remove_scenario(&cwd);
                Ok(())
            }
            Err(report) => {
                if self.keep_failed {
                    if let Ok(mut kept) = self.kept.lock() {
                        kept.push(cwd.clone());
                    }
                    Err(Failed::from(format!(
                        "{report}\n(scratch kept at {})",
                        cwd.display()
                    )))
                } else {
                    scratch::remove_scenario(&cwd);
                    Err(Failed::from(report))
                }
            }
        }
    }
}
