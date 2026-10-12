//! Scenario format v0: the `scenario.json` manifest, parsed strictly,
//! and the raw-text `# expect-invalid:` header of a negative control.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

/// The manifest file name inside a scenario directory.
pub(crate) const MANIFEST: &str = "scenario.json";

/// The header prefix a negative-control input's first line carries.
const INVALID_HEADER: &str = "# expect-invalid:";

/// The one argument placeholder: it expands to the canonical suite root,
/// so a scenario can name a committed file outside its own directory.
pub(crate) const SUITE_PLACEHOLDER: &str = "{fx-suite}";

/// A loaded scenario, ready to materialize and run.
#[derive(Debug)]
pub(crate) struct Scenario {
    pub(crate) args: Vec<String>,
    pub(crate) expectation: Expectation,
}

/// What a run must show for the scenario to pass. Harness
/// independence: the harness never asks the processor under test to
/// interpret an expectation; every assertion is a plain comparison
/// made here.
#[derive(Debug)]
pub(crate) enum Expectation {
    /// Positive control: exact exit code plus stream substrings.
    Positive {
        exit: i32,
        stdout_contains: Vec<String>,
        stderr_contains: Vec<String>,
    },
    /// Negative control: a nonzero exit whose stderr names every code.
    Invalid { input: PathBuf, codes: Vec<String> },
}

/// Load and validate a scenario from its source directory with
/// `serde_json`. Required: `binary` (`"kdlith"` only), `args`, and
/// exactly one of `expect` / `expect-invalid`; unknown keys are load
/// errors; `description` is optional.
pub(crate) fn load(dir: &Path) -> Result<Scenario, String> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{MANIFEST} is not valid JSON: {e}"))?;
    let root = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST} must be a JSON object"))?;
    check_keys(
        root,
        &["binary", "description", "args", "expect", "expect-invalid"],
        MANIFEST,
    )?;

    match root.get("binary") {
        None => return Err(format!("{MANIFEST}: missing required key \"binary\"")),
        Some(Value::String(s)) if s == "kdlith" => {}
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: \"binary\" must be \"kdlith\" (the only supported value), got {other}"
            ));
        }
    }
    if let Some(d) = root.get("description")
        && !d.is_string()
    {
        return Err(format!("{MANIFEST}: \"description\" must be a string"));
    }
    let args = match root.get("args") {
        None => return Err(format!("{MANIFEST}: missing required key \"args\"")),
        Some(v) => string_array(v, "args")?,
    };
    check_placeholders(&args)?;

    let expectation = match (root.get("expect"), root.get("expect-invalid")) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "{MANIFEST}: \"expect\" and \"expect-invalid\" are mutually exclusive"
            ));
        }
        (None, None) => {
            return Err(format!(
                "{MANIFEST}: exactly one of \"expect\" or \"expect-invalid\" is required"
            ));
        }
        (Some(v), None) => positive(v)?,
        (None, Some(v)) => invalid(dir, v)?,
    };
    Ok(Scenario { args, expectation })
}

/// Parse the positive control `expect`: optional integer `exit`
/// (default 0) plus optional `stdout-contains` and `stderr-contains`
/// string arrays.
fn positive(value: &Value) -> Result<Expectation, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: \"expect\" must be an object"))?;
    check_keys(
        obj,
        &["exit", "stdout-contains", "stderr-contains"],
        "\"expect\"",
    )?;
    let exit = match obj.get("exit") {
        None => 0,
        Some(v) => v
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| format!("{MANIFEST}: \"expect.exit\" must be an integer exit code"))?,
    };
    let stdout_contains = match obj.get("stdout-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, "expect.stdout-contains")?,
    };
    let stderr_contains = match obj.get("stderr-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, "expect.stderr-contains")?,
    };
    Ok(Expectation::Positive {
        exit,
        stdout_contains,
        stderr_contains,
    })
}

/// Parse the negative control `expect-invalid`: `input` names a
/// relative path inside the scenario whose first line declares the
/// rejected diagnostic codes.
fn invalid(dir: &Path, value: &Value) -> Result<Expectation, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: \"expect-invalid\" must be an object"))?;
    check_keys(obj, &["input"], "\"expect-invalid\"")?;
    let input = match obj.get("input") {
        None => {
            return Err(format!(
                "{MANIFEST}: \"expect-invalid\" is missing required key \"input\""
            ));
        }
        Some(Value::String(s)) => contained_relative(s)?,
        Some(_) => {
            return Err(format!(
                "{MANIFEST}: \"expect-invalid.input\" must be a string"
            ));
        }
    };
    let codes = read_invalid_header(&dir.join(&input))?;
    Ok(Expectation::Invalid { input, codes })
}

/// Read the first line of a negative-control input as raw text (no
/// parser) and extract its declared codes.
fn read_invalid_header(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read expect-invalid input {}: {e}", path.display()))?;
    let first = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    let first = std::str::from_utf8(first).map_err(|_| {
        format!(
            "{}: first line is not valid UTF-8; expected `{INVALID_HEADER} <codes>`",
            path.display()
        )
    })?;
    let first = first.strip_suffix('\r').unwrap_or(first);
    let list = first.strip_prefix(INVALID_HEADER).ok_or_else(|| {
        format!(
            "{}: first line must be `{INVALID_HEADER} <codes>`, found {first:?}",
            path.display()
        )
    })?;
    let codes: Vec<String> = list.split(',').map(|c| c.trim().to_string()).collect();
    if codes.iter().any(String::is_empty) {
        return Err(format!(
            "{}: `{INVALID_HEADER}` needs a non-empty comma-separated code list, found {list:?}",
            path.display()
        ));
    }
    Ok(codes)
}

/// A relative path that stays inside the scenario directory.
fn contained_relative(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    let contained = !raw.is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if contained {
        Ok(path)
    } else {
        Err(format!(
            "{MANIFEST}: \"expect-invalid.input\" must be a relative path inside the scenario, got {raw:?}"
        ))
    }
}

/// Reject any `{fx-` token other than the suite placeholder, so a
/// misspelled placeholder fails the load instead of reaching the binary.
fn check_placeholders(args: &[String]) -> Result<(), String> {
    for arg in args {
        let rest = arg.replace(SUITE_PLACEHOLDER, "");
        if rest.contains("{fx-") {
            return Err(format!(
                "{MANIFEST}: \"args\" entry {arg:?} has an unknown placeholder; only {SUITE_PLACEHOLDER} is supported"
            ));
        }
    }
    Ok(())
}

/// The scenario's argv with every suite placeholder expanded.
pub(crate) fn expand_args(args: &[String], suite: &Path) -> Vec<String> {
    let root = suite.to_string_lossy();
    args.iter()
        .map(|arg| arg.replace(SUITE_PLACEHOLDER, &root))
        .collect()
}

fn check_keys(obj: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("{what}: unknown key {k:?}")),
        None => Ok(()),
    }
}

fn string_array(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{MANIFEST}: \"{what}\" must be an array of strings"))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{MANIFEST}: \"{what}\" must contain only strings"))
        })
        .collect()
}
