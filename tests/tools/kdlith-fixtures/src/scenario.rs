//! Scenario format: the `scenario.json` manifest, parsed strictly, the
//! raw-text `# expect-invalid:` header of a negative control, and the
//! multi-step codegen-oracle kind.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

/// The manifest file name inside a scenario directory.
pub(crate) const MANIFEST: &str = "scenario.json";

/// The header prefix a negative-control input's first line carries.
const INVALID_HEADER: &str = "# expect-invalid:";

/// The suite placeholder: it expands to the canonical suite root, so a
/// scenario can name a committed file outside its own directory.
pub(crate) const SUITE_PLACEHOLDER: &str = "{fx-suite}";

/// The repository placeholder: it expands to the repository root above
/// the suite, so a codegen-oracle scenario can name read-only repository
/// content such as the reference processor.
pub(crate) const REPO_PLACEHOLDER: &str = "{fx-repo}";

/// A loaded scenario, ready to materialize and run.
#[derive(Debug)]
pub(crate) enum Scenario {
    /// The single-invocation shape: one processor run judged by stream
    /// assertions and an optional scratch check.
    Invoke(Invocation),
    /// The codegen-oracle shape: ordered steps over processor
    /// generation, every judgement a plain harness-side comparison.
    Oracle(CodegenOracle),
}

/// The single-invocation shape.
#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) args: Vec<String>,
    /// The `source-embedded` pairs (name, scenario directory): the run is
    /// made with each as a `--source` and as an embedded source, and the
    /// two runs must agree byte for byte.
    pub(crate) embedded: Vec<(String, String)>,
    pub(crate) expectation: Expectation,
    /// A declared scratch check: violations the harness itself must
    /// plant and then classify in the materialized copy.
    pub(crate) scratch_check: Option<ScratchCheck>,
}

/// The codegen-oracle scenario: the entrypoint, the crate name, and the
/// ordered steps the harness runs and judges.
#[derive(Debug)]
pub(crate) struct CodegenOracle {
    /// Which `gen` form drives generation.
    pub(crate) form: GenForm,
    /// The definition entrypoint, placeholders allowed.
    pub(crate) entrypoint: String,
    /// The crate name generation publishes.
    pub(crate) crate_name: String,
    /// The `NAME=DIR` named sources every generate step passes as
    /// `--source`, placeholders allowed.
    pub(crate) sources: Vec<String>,
    /// The ordered steps; never empty.
    pub(crate) steps: Vec<OracleStep>,
}

/// The `gen` form a codegen-oracle scenario drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenForm {
    Dialect,
    Schema,
}

impl GenForm {
    /// The subcommand spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            GenForm::Dialect => "dialect",
            GenForm::Schema => "schema",
        }
    }
}

/// One ordered step of a codegen-oracle run.
#[derive(Debug)]
pub(crate) enum OracleStep {
    /// Run the processor's `gen` form into `out` (relative to the
    /// materialized copy) with the declared `args` appended after the
    /// pinned argument shape; a nonzero exit fails the scenario, so a
    /// gen flag the processor refuses fails here by construction.
    Generate { out: String, args: Vec<String> },
    /// Require every named tree (relative to the materialized copy) to
    /// hold exactly the same files with exactly the same bytes.
    Determinism { trees: Vec<String> },
    /// Require the generated tree's `**/*.rs` files to match the
    /// reference tree byte for byte, excluding exactly the generated
    /// marker header line.
    Compare {
        generated: String,
        reference: String,
    },
    /// Materialize the scratch workspace (the scenario's committed
    /// template, plus the declared repository crates copied in), then
    /// run the declared command; a nonzero exit fails the scenario.
    Compile {
        workspace: String,
        copy: RepoCopy,
        command: Vec<String>,
    },
    /// Run the declared command against the compiled crate and judge
    /// its exit and stream substrings, as a positive control does.
    Exercise {
        command: Vec<String>,
        stdout_contains: Vec<String>,
        stdout_not_contains: Vec<String>,
        stderr_contains: Vec<String>,
    },
}

/// What a compile step copies from the repository into the scratch
/// workspace, at the pinned layout positions the generated crate's own
/// manifest and seed includes demand.
#[derive(Debug)]
pub(crate) struct RepoCopy {
    /// Crate directory names under `pkg/rs`, copied to `<ws>/rs/<name>`
    /// so the generated crate's `../<name>` path dependencies resolve.
    pub(crate) crates: Vec<String>,
    /// Copy `pkg/kdl` to `<ws>/kdl`, where the generated crate's seed
    /// includes point.
    pub(crate) kdl: bool,
}

impl OracleStep {
    /// The step's kind name, for failure reports.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            OracleStep::Generate { .. } => "generate",
            OracleStep::Determinism { .. } => "determinism",
            OracleStep::Compare { .. } => "compare",
            OracleStep::Compile { .. } => "compile",
            OracleStep::Exercise { .. } => "exercise",
        }
    }
}

/// A planted violation and the scratch classification it must produce.
#[derive(Debug)]
pub(crate) struct ScratchCheck {
    pub(crate) plants: Vec<Plant>,
}

/// One entry the harness plants inside the materialized scenario copy
/// after the run. The path must be foreign to the scenario source, so
/// the post-run walk classifies it as a foreign entry.
#[derive(Debug)]
pub(crate) struct Plant {
    pub(crate) path: String,
    pub(crate) kind: PlantKind,
}

#[derive(Debug)]
pub(crate) enum PlantKind {
    /// A symlink, recorded and removed as a link, never followed.
    Symlink { target: String },
    /// A regular file the scenario never declared.
    File,
}

impl PlantKind {
    /// The kind name used in classification reports.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            PlantKind::Symlink { .. } => "symlink",
            PlantKind::File => "file",
        }
    }
}

/// What a run must show for the scenario to pass. Harness
/// independence: the harness never asks the processor under test to
/// interpret an expectation; every assertion is a plain comparison
/// made here.
#[derive(Debug)]
pub(crate) enum Expectation {
    /// Positive control: exact exit code, stream substrings (required
    /// and forbidden), and the optional byte-exact goldens.
    Positive {
        exit: i32,
        stdout_contains: Vec<String>,
        stdout_not_contains: Vec<String>,
        stderr_contains: Vec<String>,
        goldens: Goldens,
    },
    /// Negative control: a nonzero exit whose stderr names every code.
    Invalid { input: PathBuf, codes: Vec<String> },
}

/// Byte-exact pins a positive control may declare. Every golden is a
/// committed file inside the scenario directory, read at load time, so
/// a missing golden fails the load rather than the comparison.
#[derive(Debug, Default)]
pub(crate) struct Goldens {
    /// The whole of stdout must equal these bytes.
    pub(crate) stdout: Option<GoldenFile>,
    /// The whole of stderr must equal these bytes.
    pub(crate) stderr: Option<GoldenFile>,
    /// Files the run leaves in the materialized copy, each required to be
    /// a regular file equal to its golden.
    pub(crate) files: Vec<(String, GoldenFile)>,
    /// Paths that must not exist in the materialized copy after the run.
    pub(crate) absent: Vec<String>,
}

/// A committed golden: its scenario-relative path and its bytes.
#[derive(Debug)]
pub(crate) struct GoldenFile {
    pub(crate) path: String,
    pub(crate) bytes: Vec<u8>,
}

/// Load and validate a scenario from its source directory with
/// `serde_json`. Required: `binary` (`"kdlith"` only), `args`, and
/// exactly one of `expect` / `expect-invalid`; unknown keys are load
/// errors; `description` is optional. A `"kind": "codegen-oracle"`
/// manifest instead selects the multi-step oracle shape with its own
/// strict key set.
pub(crate) fn load(dir: &Path) -> Result<Scenario, String> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{MANIFEST} is not valid JSON: {e}"))?;
    let root = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST} must be a JSON object"))?;
    match root.get("kind") {
        None => invoke(dir, root),
        Some(Value::String(s)) if s == "codegen-oracle" => oracle(root),
        Some(other) => Err(format!(
            "{MANIFEST}: \"kind\" must be \"codegen-oracle\" when present, got {other}"
        )),
    }
}

/// Load the single-invocation shape, whose key set is unchanged.
fn invoke(dir: &Path, root: &Map<String, Value>) -> Result<Scenario, String> {
    check_keys(
        root,
        &[
            "binary",
            "description",
            "args",
            "source-embedded",
            "expect",
            "expect-invalid",
        ],
        MANIFEST,
    )?;
    require_kdlith_binary(root)?;

    let args = match root.get("args") {
        None => return Err(format!("{MANIFEST}: missing required key \"args\"")),
        Some(v) => string_array(v, "args")?,
    };
    check_placeholders(&args)?;
    let embedded = match root.get("source-embedded") {
        None => Vec::new(),
        Some(v) => source_embedded(dir, v)?,
    };

    let (expectation, scratch_check) = match (root.get("expect"), root.get("expect-invalid")) {
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
        (Some(v), None) => positive(dir, v)?,
        (None, Some(v)) => (invalid(dir, v)?, None),
    };
    Ok(Scenario::Invoke(Invocation {
        args,
        embedded,
        expectation,
        scratch_check,
    }))
}

/// Load the codegen-oracle shape.
fn oracle(root: &Map<String, Value>) -> Result<Scenario, String> {
    check_keys(
        root,
        &[
            "binary",
            "description",
            "kind",
            "form",
            "entrypoint",
            "crate",
            "sources",
            "steps",
        ],
        MANIFEST,
    )?;
    require_kdlith_binary(root)?;
    if let Some(d) = root.get("description")
        && !d.is_string()
    {
        return Err(format!("{MANIFEST}: \"description\" must be a string"));
    }
    let form = match root.get("form") {
        None => GenForm::Dialect,
        Some(Value::String(s)) if s == "dialect" => GenForm::Dialect,
        Some(Value::String(s)) if s == "schema" => GenForm::Schema,
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: \"form\" must be \"dialect\" or \"schema\", got {other}"
            ));
        }
    };
    let entrypoint = required_string(root, "entrypoint")?;
    check_placeholder_string(&entrypoint, "\"entrypoint\"")?;
    let crate_name = required_string(root, "crate")?;
    if crate_name.is_empty() {
        return Err(format!("{MANIFEST}: \"crate\" must not be empty"));
    }
    let sources = match root.get("sources") {
        None => Vec::new(),
        Some(v) => string_array(v, "sources")?,
    };
    if root.contains_key("sources") && (form != GenForm::Schema || sources.is_empty()) {
        return Err(format!(
            "{MANIFEST}: \"sources\" is a non-empty list, and only the schema form takes it"
        ));
    }
    for source in &sources {
        if !source
            .split_once('=')
            .is_some_and(|(n, d)| !n.is_empty() && !d.is_empty())
        {
            return Err(format!(
                "{MANIFEST}: \"sources\" entries take NAME=DIR, not {source:?}"
            ));
        }
        check_placeholder_string(source, "\"sources\" entry")?;
    }
    let steps_value = match root.get("steps") {
        Some(v) => v
            .as_array()
            .ok_or_else(|| format!("{MANIFEST}: \"steps\" must be an array"))?,
        None => return Err(format!("{MANIFEST}: missing required key \"steps\"")),
    };
    if steps_value.is_empty() {
        return Err(format!(
            "{MANIFEST}: \"steps\" must not be empty; a zero-step run never passes"
        ));
    }
    let steps = steps_value
        .iter()
        .enumerate()
        .map(|(i, v)| oracle_step(i, v))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Scenario::Oracle(CodegenOracle {
        form,
        entrypoint,
        crate_name,
        sources,
        steps,
    }))
}

/// The step kinds a codegen-oracle manifest may declare.
const STEP_KINDS: &[&str] = &["generate", "determinism", "compare", "compile", "exercise"];

/// Parse one entry of the `steps` array: an object carrying exactly
/// one step kind and that kind's declared shape.
fn oracle_step(index: usize, value: &Value) -> Result<OracleStep, String> {
    let what = format!("steps[{index}]");
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} must be an object"))?;
    check_keys(obj, STEP_KINDS, &what)?;
    let declared = obj
        .keys()
        .filter(|k| STEP_KINDS.contains(&k.as_str()))
        .count();
    if declared != 1 {
        return Err(format!(
            "{MANIFEST}: {what} must carry exactly one of {}",
            STEP_KINDS.join(", ")
        ));
    }
    let mismatch = |kind: &str, want: &str| format!("{MANIFEST}: {what} {kind} must be {want}");
    if let Some(v) = obj.get("generate") {
        let (out, args) = match v {
            Value::String(out) => (
                contained(out, &format!("{what} generate output"))?,
                Vec::new(),
            ),
            Value::Object(spec) => {
                check_keys(spec, &["out", "args"], &format!("{what} generate"))?;
                let out = required_string(spec, "out")?;
                let out = contained(&out, &format!("{what} generate output"))?;
                let args = match spec.get("args") {
                    None => Vec::new(),
                    Some(list) => {
                        let args = string_array(list, &format!("{what} generate args"))?;
                        if args.is_empty() {
                            return Err(format!(
                                "{MANIFEST}: {what} generate args must not be empty"
                            ));
                        }
                        for arg in &args {
                            check_placeholder_string(arg, &format!("{what} generate args entry"))?;
                        }
                        args
                    }
                };
                (out, args)
            }
            other => {
                return Err(format!(
                    "{MANIFEST}: {what} generate must be a string output directory or an object with \"out\", got {other}"
                ));
            }
        };
        return Ok(OracleStep::Generate { out, args });
    }
    if let Some(v) = obj.get("determinism") {
        let list = v
            .as_array()
            .ok_or_else(|| mismatch("determinism", "an array of tree paths"))?;
        if list.len() < 2 {
            return Err(format!(
                "{MANIFEST}: {what} determinism needs at least two trees to compare"
            ));
        }
        let mut trees = Vec::with_capacity(list.len());
        for item in list {
            let tree = item
                .as_str()
                .ok_or_else(|| mismatch("determinism", "an array of tree paths"))?;
            trees.push(contained(tree, &format!("{what} determinism tree"))?);
        }
        return Ok(OracleStep::Determinism { trees });
    }
    if let Some(v) = obj.get("compare") {
        let obj = v
            .as_object()
            .ok_or_else(|| mismatch("compare", "an object"))?;
        check_keys(obj, &["generated", "reference"], &format!("{what} compare"))?;
        let generated = required_string(obj, "generated")?;
        let generated = contained(&generated, &format!("{what} compare generated tree"))?;
        let reference = required_string(obj, "reference")?;
        if reference.is_empty() {
            return Err(format!(
                "{MANIFEST}: {what} compare reference must not be empty"
            ));
        }
        check_placeholder_string(&reference, &format!("{what} compare reference"))?;
        return Ok(OracleStep::Compare {
            generated,
            reference,
        });
    }
    if let Some(v) = obj.get("compile") {
        let obj = v
            .as_object()
            .ok_or_else(|| mismatch("compile", "an object"))?;
        check_keys(
            obj,
            &["workspace", "copy", "command"],
            &format!("{what} compile"),
        )?;
        let workspace = required_string(obj, "workspace")?;
        let workspace = contained(&workspace, &format!("{what} compile workspace"))?;
        let copy = match obj.get("copy") {
            Some(v) => v
                .as_object()
                .ok_or_else(|| mismatch("compile copy", "an object"))?,
            None => return Err(format!("{MANIFEST}: {what} compile is missing \"copy\"")),
        };
        check_keys(copy, &["crates", "kdl"], &format!("{what} compile copy"))?;
        let crates_value = match copy.get("crates") {
            Some(v) => v
                .as_array()
                .ok_or_else(|| mismatch("compile copy crates", "an array of crate names"))?,
            None => {
                return Err(format!(
                    "{MANIFEST}: {what} compile copy is missing \"crates\""
                ));
            }
        };
        if crates_value.is_empty() {
            return Err(format!(
                "{MANIFEST}: {what} compile copy crates must not be empty"
            ));
        }
        let mut crates = Vec::with_capacity(crates_value.len());
        for item in crates_value {
            let name = item
                .as_str()
                .ok_or_else(|| mismatch("compile copy crates", "an array of crate names"))?;
            let plain = !name.is_empty()
                && Path::new(name).components().next().is_some_and(|c| {
                    matches!(c, Component::Normal(_)) && name.chars().all(|ch| ch != '/')
                })
                && !name.contains('\\');
            if !plain {
                return Err(format!(
                    "{MANIFEST}: {what} compile copy crate {name:?} must be one plain directory name under pkg/rs"
                ));
            }
            crates.push(name.to_owned());
        }
        let kdl = match copy.get("kdl") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(format!(
                    "{MANIFEST}: {what} compile copy kdl must be a boolean"
                ));
            }
        };
        let command = required_command(obj, &format!("{what} compile"))?;
        return Ok(OracleStep::Compile {
            workspace,
            copy: RepoCopy { crates, kdl },
            command,
        });
    }
    let v = obj
        .get("exercise")
        .ok_or_else(|| format!("{MANIFEST}: {what} carries no step kind"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| mismatch("exercise", "an object"))?;
    check_keys(
        obj,
        &[
            "command",
            "stdout-contains",
            "stdout-not-contains",
            "stderr-contains",
        ],
        &format!("{what} exercise"),
    )?;
    let command = required_command(obj, &format!("{what} exercise"))?;
    let stdout_contains = match obj.get("stdout-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, &format!("{what} exercise stdout-contains"))?,
    };
    let stdout_not_contains = match obj.get("stdout-not-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, &format!("{what} exercise stdout-not-contains"))?,
    };
    let stderr_contains = match obj.get("stderr-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, &format!("{what} exercise stderr-contains"))?,
    };
    Ok(OracleStep::Exercise {
        command,
        stdout_contains,
        stdout_not_contains,
        stderr_contains,
    })
}

/// The shared `binary` contract: required, and `"kdlith"` only.
fn require_kdlith_binary(root: &Map<String, Value>) -> Result<(), String> {
    match root.get("binary") {
        None => return Err(format!("{MANIFEST}: missing required key \"binary\"")),
        Some(Value::String(s)) if s == "kdlith" => {}
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: \"binary\" must be \"kdlith\" (the only supported value), got {other}"
            ));
        }
    }
    Ok(())
}

/// A required string value under `key`.
fn required_string(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    match obj.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!("{MANIFEST}: {key:?} must be a string, got {other}")),
        None => Err(format!("{MANIFEST}: missing required key {key:?}")),
    }
}

/// A required non-empty string-array command whose entries carry only
/// the known placeholders.
fn required_command(obj: &Map<String, Value>, what: &str) -> Result<Vec<String>, String> {
    let command = match obj.get("command") {
        Some(v) => string_array(v, &format!("{what} command"))?,
        None => return Err(format!("{MANIFEST}: {what} is missing \"command\"")),
    };
    if command.is_empty() {
        return Err(format!("{MANIFEST}: {what} command must not be empty"));
    }
    check_placeholders(&command)?;
    Ok(command)
}

/// Parse the positive control `expect`: exit code, required and
/// forbidden stream substrings, byte-exact goldens, and an optional
/// scratch check.
fn positive(dir: &Path, value: &Value) -> Result<(Expectation, Option<ScratchCheck>), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: \"expect\" must be an object"))?;
    check_keys(
        obj,
        &[
            "exit",
            "stdout-contains",
            "stdout-not-contains",
            "stderr-contains",
            "stdout-golden",
            "stderr-golden",
            "files",
            "absent",
            "scratch-check",
        ],
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
    let stdout_not_contains = match obj.get("stdout-not-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, "expect.stdout-not-contains")?,
    };
    let stderr_contains = match obj.get("stderr-contains") {
        None => Vec::new(),
        Some(v) => string_array(v, "expect.stderr-contains")?,
    };
    let scratch_check = match obj.get("scratch-check") {
        None => None,
        Some(v) => Some(scratch_check(dir, v)?),
    };
    let goldens = goldens(dir, obj)?;
    Ok((
        Expectation::Positive {
            exit,
            stdout_contains,
            stdout_not_contains,
            stderr_contains,
            goldens,
        },
        scratch_check,
    ))
}

/// Parse the byte-exact pins: stream goldens, produced files mapped to
/// goldens, and paths that must stay absent, all inside the scenario.
fn goldens(dir: &Path, obj: &Map<String, Value>) -> Result<Goldens, String> {
    let stream = |key: &str| -> Result<Option<GoldenFile>, String> {
        match obj.get(key) {
            None => Ok(None),
            Some(Value::String(s)) => golden_file(dir, s, &format!("expect.{key}")).map(Some),
            Some(_) => Err(format!("{MANIFEST}: \"expect.{key}\" must be a string")),
        }
    };
    let stdout = stream("stdout-golden")?;
    let stderr = stream("stderr-golden")?;
    let mut files = Vec::new();
    if let Some(v) = obj.get("files") {
        let map = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: \"expect.files\" must be an object"))?;
        if map.is_empty() {
            return Err(format!("{MANIFEST}: \"expect.files\" must not be empty"));
        }
        for (produced, golden) in map {
            contained(produced, "\"expect.files\" key")?;
            let golden = golden.as_str().ok_or_else(|| {
                format!("{MANIFEST}: \"expect.files\" values must be golden file paths")
            })?;
            files.push((produced.clone(), golden_file(dir, golden, "expect.files")?));
        }
    }
    let absent = match obj.get("absent") {
        None => Vec::new(),
        Some(v) => {
            let list = string_array(v, "expect.absent")?;
            if list.is_empty() {
                return Err(format!("{MANIFEST}: \"expect.absent\" must not be empty"));
            }
            for path in &list {
                contained(path, "\"expect.absent\" entry")?;
            }
            list
        }
    };
    Ok(Goldens {
        stdout,
        stderr,
        files,
        absent,
    })
}

/// Read a committed golden inside the scenario directory.
fn golden_file(dir: &Path, raw: &str, what: &str) -> Result<GoldenFile, String> {
    let path = contained(raw, &format!("\"{what}\" golden"))?;
    let bytes = std::fs::read(dir.join(&path))
        .map_err(|e| format!("{MANIFEST}: cannot read {what} golden {path:?}: {e}"))?;
    Ok(GoldenFile { path, bytes })
}

/// Parse the `scratch-check` block: a non-empty `plant` array whose
/// entries name a contained, not-yet-existing path and a plant kind.
fn scratch_check(dir: &Path, value: &Value) -> Result<ScratchCheck, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: \"expect.scratch-check\" must be an object"))?;
    check_keys(obj, &["plant"], "\"expect.scratch-check\"")?;
    let list = match obj.get("plant") {
        None => {
            return Err(format!(
                "{MANIFEST}: \"expect.scratch-check\" is missing required key \"plant\""
            ));
        }
        Some(v) => v.as_array().ok_or_else(|| {
            format!("{MANIFEST}: \"expect.scratch-check.plant\" must be an array")
        })?,
    };
    if list.is_empty() {
        return Err(format!(
            "{MANIFEST}: \"expect.scratch-check.plant\" must not be empty"
        ));
    }
    let mut plants = Vec::new();
    for item in list {
        let plant = item
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: each plant must be an object"))?;
        check_keys(plant, &["path", "kind", "target"], "plant")?;
        let path = match plant.get("path") {
            Some(Value::String(s)) => s.clone(),
            Some(other) => {
                return Err(format!(
                    "{MANIFEST}: plant \"path\" must be a string, got {other}"
                ));
            }
            None => {
                return Err(format!(
                    "{MANIFEST}: plant is missing required key \"path\""
                ));
            }
        };
        contained(&path, "plant path")?;
        if dir.join(&path).symlink_metadata().is_ok() {
            return Err(format!(
                "{MANIFEST}: planted entry {path:?} already exists in the scenario; a planted violation must be foreign to it"
            ));
        }
        let kind = match plant.get("kind") {
            Some(Value::String(s)) if s == "symlink" || s == "file" => s.clone(),
            Some(other) => {
                return Err(format!(
                    "{MANIFEST}: plant \"kind\" must be \"symlink\" or \"file\", got {other}"
                ));
            }
            None => {
                return Err(format!(
                    "{MANIFEST}: plant is missing required key \"kind\""
                ));
            }
        };
        let kind = if kind == "symlink" {
            match plant.get("target") {
                Some(Value::String(t)) if !t.is_empty() => PlantKind::Symlink { target: t.clone() },
                Some(_) => {
                    return Err(format!(
                        "{MANIFEST}: plant \"target\" must be a non-empty string"
                    ));
                }
                None => {
                    return Err(format!(
                        "{MANIFEST}: a \"symlink\" plant requires a \"target\""
                    ));
                }
            }
        } else if let Some(target) = plant.get("target") {
            return Err(format!(
                "{MANIFEST}: plant key {target} is only allowed for a \"symlink\" plant"
            ));
        } else {
            PlantKind::File
        };
        plants.push(Plant { path, kind });
    }
    Ok(ScratchCheck { plants })
}

/// Parse `source-embedded`: a non-empty array of `NAME=DIR` strings,
/// each NAME a distinct non-empty kebab name and each DIR a directory
/// inside the scenario.
fn source_embedded(dir: &Path, value: &Value) -> Result<Vec<(String, String)>, String> {
    let list = string_array(value, "source-embedded")?;
    if list.is_empty() {
        return Err(format!("{MANIFEST}: \"source-embedded\" must not be empty"));
    }
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(list.len());
    for spec in list {
        let Some((name, path)) = spec.split_once('=') else {
            return Err(format!(
                "{MANIFEST}: \"source-embedded\" entries take NAME=DIR, not {spec:?}"
            ));
        };
        let kebab = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !kebab || pairs.iter().any(|(seen, _)| seen == name) {
            return Err(format!(
                "{MANIFEST}: \"source-embedded\" name {name:?} must be a distinct kebab name"
            ));
        }
        let path = contained(path, "\"source-embedded\" directory")?;
        if !dir.join(&path).is_dir() {
            return Err(format!(
                "{MANIFEST}: \"source-embedded\" directory {path:?} is not a directory of the scenario"
            ));
        }
        pairs.push((name.to_owned(), path));
    }
    Ok(pairs)
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
        Some(Value::String(s)) => {
            let path = contained(s, "\"expect-invalid.input\"")?;
            PathBuf::from(path)
        }
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
fn contained(raw: &str, what: &str) -> Result<String, String> {
    let path = PathBuf::from(raw);
    let inside = !raw.is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if inside {
        Ok(raw.to_owned())
    } else {
        Err(format!(
            "{MANIFEST}: {what} must be a relative path inside the scenario, got {raw:?}"
        ))
    }
}

/// Reject any `{fx-` token other than the known placeholders, so a
/// misspelled placeholder fails the load instead of reaching a command.
fn check_placeholders(args: &[String]) -> Result<(), String> {
    for arg in args {
        check_placeholder_string(arg, "\"args\" entry")?;
    }
    Ok(())
}

fn check_placeholder_string(raw: &str, what: &str) -> Result<(), String> {
    let rest = raw
        .replace(SUITE_PLACEHOLDER, "")
        .replace(REPO_PLACEHOLDER, "");
    if rest.contains("{fx-") {
        return Err(format!(
            "{MANIFEST}: {what} {raw:?} has an unknown placeholder; only {SUITE_PLACEHOLDER} and {REPO_PLACEHOLDER} are supported"
        ));
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
