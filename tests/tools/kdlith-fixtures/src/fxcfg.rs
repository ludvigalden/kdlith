//! The harness invocation: `--fx-*` options are parsed out of argv; every
//! other argument is handed to the libtest-mimic trial runner unchanged.

use std::path::PathBuf;

/// The default scratch root, relative to the working directory.
pub(crate) const DEFAULT_SCRATCH_ROOT: &str = "target/scratch";

/// The parsed `--fx-*` options.
#[derive(Debug, Default)]
pub(crate) struct FxOptions {
    pub(crate) suite: Option<PathBuf>,
    pub(crate) kdlith: Option<PathBuf>,
    pub(crate) scratch_root: Option<PathBuf>,
    pub(crate) filter: Option<String>,
    pub(crate) list: bool,
    pub(crate) keep_failed: bool,
}

/// Split argv into the harness options and the trial-runner arguments
/// (the executable name stays first). An unknown `--fx-*` option or a
/// value-taking option without its value is a usage error.
pub(crate) fn parse(argv: &[String]) -> Result<(FxOptions, Vec<String>), String> {
    let mut opts = FxOptions::default();
    let mut rest: Vec<String> = Vec::with_capacity(argv.len());
    let mut iter = argv.iter();
    if let Some(argv0) = iter.next() {
        rest.push(argv0.clone());
    }
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--fx-") {
            rest.push(arg.clone());
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        match name {
            "--fx-list" | "--fx-keep-failed" => {
                if inline.is_some() {
                    return Err(format!("{name} takes no value"));
                }
                if name == "--fx-list" {
                    opts.list = true;
                } else {
                    opts.keep_failed = true;
                }
            }
            "--fx-suite" | "--fx-kdlith" | "--fx-scratch-root" | "--fx-filter" => {
                let value = match inline {
                    Some(v) => v,
                    None => iter
                        .next()
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a value"))?,
                };
                if value.is_empty() {
                    return Err(format!("{name} needs a non-empty value"));
                }
                let slot_taken = match name {
                    "--fx-suite" => opts.suite.replace(PathBuf::from(value)).is_some(),
                    "--fx-kdlith" => opts.kdlith.replace(PathBuf::from(value)).is_some(),
                    "--fx-scratch-root" => {
                        opts.scratch_root.replace(PathBuf::from(value)).is_some()
                    }
                    _ => opts.filter.replace(value).is_some(),
                };
                if slot_taken {
                    return Err(format!("{name} given more than once"));
                }
            }
            other => return Err(format!("unknown harness option {other}")),
        }
    }
    Ok((opts, rest))
}
