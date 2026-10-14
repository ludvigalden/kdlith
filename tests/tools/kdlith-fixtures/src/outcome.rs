//! Running one materialized scenario and judging the run against its
//! declared expectation. Every judgement is a plain comparison made by the
//! harness; the processor's output is evidence, never an interpreter.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::fd::OwnedFd;

use crate::fxbin::{ENV_KDLITH, ENV_SCENARIO, ENV_SCRATCH, ENV_SUITE};
use crate::scenario::{Expectation, Invocation, PlantKind, ScratchCheck, expand_args};
use crate::walk::{self, Expected, RelPath};

/// The content of every planted foreign file.
const PLANTED_FILE: &[u8] = b"kdlith-fixtures planted foreign file\n";

/// The paths a run exports to the processor's environment.
pub(crate) struct RunEnv<'a> {
    pub(crate) kdlith: &'a Path,
    pub(crate) suite: &'a Path,
    pub(crate) scratch: &'a Path,
    pub(crate) bin_dir: &'a Path,
    /// The run root, opened once with `walk::open_root`; every scratch
    /// classification walks below it without following a link.
    pub(crate) root_fd: &'a OwnedFd,
}

/// Run the processor in `cwd` with the invocation's argv and judge it.
/// `source` is the scenario's committed source directory, the baseline
/// the materialized copy is classified against. `Err` carries the full
/// failure report.
pub(crate) fn execute(
    invocation: &Invocation,
    cwd: &Path,
    env: &RunEnv<'_>,
    source: &Path,
) -> Result<(), String> {
    let mut path = OsString::from(env.bin_dir.as_os_str());
    if let Some(inherited) = std::env::var_os("PATH") {
        path.push(":");
        path.push(inherited);
    }
    let output = Command::new(env.kdlith)
        .args(expand_args(&invocation.args, env.suite))
        .current_dir(cwd)
        .env("PATH", path)
        .env(ENV_SUITE, env.suite)
        .env(ENV_KDLITH, env.kdlith)
        .env(ENV_SCRATCH, env.scratch)
        .env(ENV_SCENARIO, cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", env.kdlith.display()))?;
    let mut failures = judge(&invocation.expectation, &output);
    if let Some(check) = &invocation.scratch_check {
        failures.extend(scratch_check(check, cwd, env, source));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(report(&failures, &output))
    }
}

/// Every unmet assertion, in declaration order.
fn judge(expectation: &Expectation, output: &Output) -> Vec<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code();
    let mut failures = Vec::new();
    match expectation {
        Expectation::Positive {
            exit,
            stdout_contains,
            stderr_contains,
        } => {
            if code != Some(*exit) {
                failures.push(format!("exit: expected {exit}, got {}", describe(code)));
            }
            for needle in stdout_contains {
                if !stdout.contains(needle.as_str()) {
                    failures.push(format!("stdout does not contain {needle:?}"));
                }
            }
            for needle in stderr_contains {
                if !stderr.contains(needle.as_str()) {
                    failures.push(format!("stderr does not contain {needle:?}"));
                }
            }
        }
        Expectation::Invalid { input, codes } => {
            if !matches!(code, Some(c) if c != 0) {
                failures.push(format!(
                    "expect-invalid {}: expected a nonzero exit, got {}",
                    input.display(),
                    describe(code)
                ));
            }
            for c in codes {
                if !stderr.contains(c.as_str()) {
                    failures.push(format!(
                        "expect-invalid {}: declared code {c} not found in stderr",
                        input.display()
                    ));
                }
            }
        }
    }
    failures
}

fn describe(code: Option<i32>) -> String {
    match code {
        Some(c) => c.to_string(),
        None => "termination by signal".to_string(),
    }
}

fn report(failures: &[String], output: &Output) -> String {
    let mut text = String::new();
    for f in failures {
        text.push_str("  - ");
        text.push_str(f);
        text.push('\n');
    }
    text.push_str("--- stdout ---\n");
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str("\n--- stderr ---\n");
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

/// The declared scratch check: plant the declared violations inside
/// the materialized copy, then classify the copy against the scenario
/// source with the no-follow walk. Every planted entry must be
/// reported as a foreign entry of its planted kind, nothing else may
/// be reported, and the captured tree must hold each plant's target,
/// content, size, and a fresh mtime.
fn scratch_check(check: &ScratchCheck, cwd: &Path, env: &RunEnv<'_>, source: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    // One second of slack: inode timestamps come from the kernel's
    // coarse clock and can trail the realtime clock by a tick, so a
    // fresh entry may carry an mtime just before `now`.
    let planted_at = mtime_floor(
        SystemTime::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or(UNIX_EPOCH),
    );
    for plant in &check.plants {
        let path = cwd.join(&plant.path);
        let planted = match &plant.kind {
            PlantKind::Symlink { target } => std::os::unix::fs::symlink(target, &path),
            PlantKind::File => std::fs::write(&path, PLANTED_FILE),
        };
        if let Err(e) = planted {
            failures.push(format!(
                "scratch check: cannot plant {} {}: {e}",
                plant.kind.as_str(),
                plant.path
            ));
        }
    }
    let mut expected = BTreeMap::new();
    if let Err(msg) = collect_expected(source, "", &mut expected) {
        failures.push(format!("scratch check: {msg}"));
        return failures;
    }
    let rel = match cwd
        .strip_prefix(env.scratch)
        .ok()
        .and_then(|r| r.to_str())
        .map(str::to_string)
    {
        Some(rel) if !rel.is_empty() => rel,
        _ => {
            failures.push("scratch check: the scenario directory is not under the run root".into());
            return failures;
        }
    };
    let rel = match RelPath::new(&rel) {
        Ok(rel) => rel,
        Err(msg) => {
            failures.push(format!("scratch check: {msg}"));
            return failures;
        }
    };
    let mut wanted: Vec<String> = check
        .plants
        .iter()
        .map(|p| format!("foreign entry '{}' ({})", p.path, p.kind.as_str()))
        .collect();
    for violation in walk::classify(env.root_fd, &rel, &expected) {
        match wanted.iter().position(|w| *w == violation) {
            Some(hit) => {
                wanted.remove(hit);
            }
            None => failures.push(format!(
                "scratch check: unexpected classification: {violation}"
            )),
        }
    }
    for missed in &wanted {
        failures.push(format!(
            "scratch check: planted violation was not reported: {missed}"
        ));
    }
    match walk::capture_tree(env.root_fd, &rel) {
        Ok(tree) => {
            for plant in &check.plants {
                let entry_rel = format!("{}/{}", rel.as_str(), plant.path);
                let Some(entry) = tree.iter().find(|e| e.rel == entry_rel) else {
                    failures.push(format!(
                        "scratch check: planted {} '{}' is missing from the captured tree",
                        plant.kind.as_str(),
                        plant.path
                    ));
                    continue;
                };
                match &plant.kind {
                    PlantKind::Symlink { target } => {
                        if entry.link_target.as_deref() != Some(target.as_bytes()) {
                            failures.push(format!(
                                "scratch check: planted symlink '{}' target was not captured without following it",
                                plant.path
                            ));
                        }
                    }
                    PlantKind::File => {
                        if entry.bytes.as_deref() != Some(PLANTED_FILE)
                            || entry.size != u64::try_from(PLANTED_FILE.len()).unwrap_or(u64::MAX)
                        {
                            failures.push(format!(
                                "scratch check: planted file '{}' content or size was not captured",
                                plant.path
                            ));
                        }
                    }
                }
                if entry.mtime < planted_at {
                    failures.push(format!(
                        "scratch check: planted entry '{}' predates the run",
                        plant.path
                    ));
                }
            }
        }
        Err(e) => failures.push(format!("scratch check: {}", e.text())),
    }
    failures
}

/// Enumerate the scenario source as the expected scratch baseline:
/// every file and directory the materialized copy legitimately holds.
fn collect_expected(
    dir: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, Expected>,
) -> Result<(), String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut items: Vec<_> = read
        .collect::<Result<_, _>>()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let name = item.file_name().to_string_lossy().to_string();
        let key = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = item
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", item.path().display()))?;
        if file_type.is_dir() {
            out.insert(
                key.clone(),
                Expected {
                    kind: walk::Kind::Dir,
                    mode: None,
                },
            );
            collect_expected(&item.path(), &key, out)?;
        } else if file_type.is_file() {
            out.insert(
                key,
                Expected {
                    kind: walk::Kind::File,
                    mode: None,
                },
            );
        } else {
            return Err(format!(
                "{} holds a link or special file; the baseline must be plain files and directories",
                item.path().display()
            ));
        }
    }
    Ok(())
}

/// A `SystemTime` as the `(secs, nanos)` pair the walk reports, so a
/// captured mtime can be compared against the moment of planting.
fn mtime_floor(t: SystemTime) -> (i64, u32) {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => (
            i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
            d.subsec_nanos(),
        ),
        Err(e) => {
            let secs = i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX);
            (secs.checked_neg().unwrap_or(i64::MIN), 0)
        }
    }
}
