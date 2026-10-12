//! Running one materialized scenario and judging the run against its
//! declared expectation. Every judgement is a plain comparison made by the
//! harness; the processor's output is evidence, never an interpreter.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::fxbin::{ENV_KDLITH, ENV_SCENARIO, ENV_SCRATCH, ENV_SUITE};
use crate::scenario::{Expectation, Scenario};

/// The paths a run exports to the processor's environment.
pub(crate) struct RunEnv<'a> {
    pub(crate) kdlith: &'a Path,
    pub(crate) suite: &'a Path,
    pub(crate) scratch: &'a Path,
    pub(crate) bin_dir: &'a Path,
}

/// Run the processor in `cwd` with the scenario's argv and judge it.
/// `Err` carries the full failure report.
pub(crate) fn execute(scenario: &Scenario, cwd: &Path, env: &RunEnv<'_>) -> Result<(), String> {
    let mut path = OsString::from(env.bin_dir.as_os_str());
    if let Some(inherited) = std::env::var_os("PATH") {
        path.push(":");
        path.push(inherited);
    }
    let output = Command::new(env.kdlith)
        .args(&scenario.args)
        .current_dir(cwd)
        .env("PATH", path)
        .env(ENV_SUITE, env.suite)
        .env(ENV_KDLITH, env.kdlith)
        .env(ENV_SCRATCH, env.scratch)
        .env(ENV_SCENARIO, cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", env.kdlith.display()))?;
    let failures = judge(&scenario.expectation, &output);
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
