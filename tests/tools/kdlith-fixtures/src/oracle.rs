//! The codegen-oracle runner: ordered steps over processor generation,
//! each judged by a plain harness-side comparison. The processor under
//! test generates; the harness compares, compiles, and exercises.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use kdlith_codegen::RUST_MARKER;

use crate::fxbin::{ENV_KDLITH, ENV_REPO, ENV_SCENARIO, ENV_SCRATCH, ENV_SUITE};
use crate::outcome::RunEnv;
use crate::scenario::{CodegenOracle, OracleStep, REPO_PLACEHOLDER, RepoCopy, SUITE_PLACEHOLDER};
use crate::scratch;

/// The cap on captured command streams echoed into a failure report.
const STREAM_CAP: usize = 8_000;

/// Run every declared step in order. The first failing step stops the
/// scenario with a report naming the step and the difference.
pub(crate) fn run(scenario: &CodegenOracle, cwd: &Path, env: &RunEnv<'_>) -> Result<(), String> {
    let repo = locate_repo(env.suite)?;
    let expansion = Expansion { repo: &repo };
    for (index, step) in scenario.steps.iter().enumerate() {
        if let Err(failure) = run_step(step, scenario, cwd, env, &expansion) {
            return Err(format!(
                "oracle step {} ({}): {failure}",
                index + 1,
                step.kind()
            ));
        }
    }
    Ok(())
}

/// Resolve the repository root: an explicit `KDLITH_FX_REPO` override
/// when it names a root holding the workspace manifest and the
/// reference processor, else the nearest such ancestor of the suite.
/// The override keeps runs under an external scratch root working —
/// the suite copy may sit below no repository at all — and a value
/// that fails the check is a loud failure, never a silent fallback
/// that could mask a stale inherited root.
fn locate_repo(suite: &Path) -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os(ENV_REPO) {
        let root = PathBuf::from(&value);
        return if is_repo_root(&root) {
            Ok(root)
        } else {
            Err(format!(
                "{ENV_REPO} names {}, which is not a repository root holding Cargo.toml and pkg/rs/superproc/Cargo.toml",
                root.display()
            ))
        };
    }
    let mut dir = suite.to_path_buf();
    loop {
        if is_repo_root(&dir) {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!(
                "cannot locate the repository root above the suite {}",
                suite.display()
            ));
        }
    }
}

fn is_repo_root(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
        && dir
            .join("pkg")
            .join("rs")
            .join("superproc")
            .join("Cargo.toml")
            .is_file()
}

/// Placeholder expansion against the suite root and the repository
/// root, resolved once per oracle run.
struct Expansion<'a> {
    repo: &'a Path,
}

impl Expansion<'_> {
    fn string(&self, raw: &str, env: &RunEnv<'_>) -> String {
        raw.replace(SUITE_PLACEHOLDER, &env.suite.to_string_lossy())
            .replace(REPO_PLACEHOLDER, &self.repo.to_string_lossy())
    }
}

fn run_step(
    step: &OracleStep,
    scenario: &CodegenOracle,
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    match step {
        OracleStep::Generate { out, args } => generate(scenario, out, args, cwd, env, expansion),
        OracleStep::Determinism { trees } => determinism(trees, cwd, env, expansion),
        OracleStep::Compare {
            generated,
            reference,
        } => compare(generated, reference, cwd, env, expansion),
        OracleStep::Compile {
            workspace,
            copy,
            command,
        } => compile(workspace, copy, command, cwd, env, expansion),
        OracleStep::Exercise {
            command,
            stdout_contains,
            stdout_not_contains,
            stderr_contains,
        } => exercise(
            command,
            stdout_contains,
            stdout_not_contains,
            stderr_contains,
            cwd,
            env,
            expansion,
        ),
    }
}

/// Run the processor's `gen` form into `out` with the declared `args`
/// appended after the pinned argument shape; a nonzero exit fails. The
/// harness holds no flag whitelist: a flag the processor refuses
/// reaches it and fails the step here.
fn generate(
    scenario: &CodegenOracle,
    out: &str,
    args: &[String],
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    let mut argv = vec![
        "gen".to_owned(),
        scenario.form.as_str().to_owned(),
        expansion.string(&scenario.entrypoint, env),
        "--crate".to_owned(),
        scenario.crate_name.clone(),
        "--out".to_owned(),
        out.to_owned(),
    ];
    for source in &scenario.sources {
        argv.push("--source".to_owned());
        argv.push(expansion.string(source, env));
    }
    for arg in args {
        argv.push(expansion.string(arg, env));
    }
    let output = spawn(env.kdlith, &argv, cwd, env, expansion)
        .map_err(|e| format!("cannot run {}: {e}", env.kdlith.display()))?;
    require_exit(
        &output,
        &format!("gen {} {:?}", scenario.form.as_str(), out),
    )
}

/// Require every named tree to hold exactly the same files with the
/// same bytes; any difference names its path.
fn determinism(
    trees: &[String],
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    let first = tree_files(&cwd.join(expansion.string(&trees[0], env)))
        .map_err(|e| format!("{}: {e}", trees[0]))?;
    for tree in &trees[1..] {
        let other = tree_files(&cwd.join(expansion.string(tree, env)))
            .map_err(|e| format!("{}: {e}", tree))?;
        if let Some(path) = first.keys().find(|p| !other.contains_key(*p)) {
            return Err(format!(
                "{} holds {path} which {} does not generate",
                trees[0], tree
            ));
        }
        if let Some(path) = other.keys().find(|p| !first.contains_key(*p)) {
            return Err(format!(
                "{} holds {path} which {} does not generate",
                tree, trees[0]
            ));
        }
        for (path, bytes) in &other {
            let baseline = &first[path];
            if baseline != bytes {
                return Err(format!(
                    "{path} differs between {} and {} at byte {} ({} vs {} bytes)",
                    trees[0],
                    tree,
                    first_diff(baseline, bytes),
                    baseline.len(),
                    bytes.len()
                ));
            }
        }
    }
    Ok(())
}

/// Require the generated tree's `**/*.rs` files to match the reference
/// tree byte for byte, excluding exactly the generated marker header
/// line — the marker, its newline, and the blank separator.
fn compare(
    generated: &str,
    reference: &str,
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    let gen_dir = cwd.join(expansion.string(generated, env));
    let reference_dir = expansion.string(reference, env);
    let generated_files = rust_files(&gen_dir, generated)?;
    let reference_files = rust_files(Path::new(&reference_dir), reference)?;
    if let Some(path) = reference_files
        .keys()
        .find(|p| !generated_files.contains_key(*p))
    {
        return Err(format!(
            "the reference holds {path} which the generated tree does not"
        ));
    }
    if let Some(path) = generated_files
        .keys()
        .find(|p| !reference_files.contains_key(*p))
    {
        return Err(format!(
            "the generated tree holds {path} which the reference does not"
        ));
    }
    let marker = format!("{RUST_MARKER}\n\n");
    for (path, bytes) in &generated_files {
        let body = bytes
            .strip_prefix(marker.as_bytes())
            .ok_or_else(|| format!("{path} does not start with the generated marker line"))?;
        let oracle = &reference_files[path];
        if body != oracle.as_slice() {
            return Err(format!(
                "{path} differs from the reference at byte {} of the marker-excluded body ({} vs {} bytes)",
                first_diff(body, oracle),
                body.len(),
                oracle.len()
            ));
        }
    }
    Ok(())
}

/// Materialize the scratch workspace and run the declared command. The
/// scenario's committed template is already in the working copy; the
/// declared repository crates are copied in first, at the pinned
/// layout positions.
fn compile(
    workspace: &str,
    copy: &RepoCopy,
    command: &[String],
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    let ws = cwd.join(expansion.string(workspace, env));
    if !ws.is_dir() {
        return Err(format!(
            "workspace template {workspace} is missing from the materialized copy"
        ));
    }
    let rs = ws.join("rs");
    std::fs::create_dir_all(&rs).map_err(|e| format!("cannot create {}: {e}", rs.display()))?;
    for name in &copy.crates {
        let source = expansion.repo.join("pkg").join("rs").join(name);
        if !source.is_dir() {
            return Err(format!(
                "repository crate {} is missing at {}",
                name,
                source.display()
            ));
        }
        let target = rs.join(name);
        if target.symlink_metadata().is_ok() {
            return Err(format!(
                "{} already exists in the workspace; the template must not carry repository crates",
                target.display()
            ));
        }
        std::fs::create_dir(&target)
            .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
        scratch::copy_tree(&source, &target)
            .map_err(|e| format!("cannot copy crate {name} into the workspace: {e}"))?;
    }
    if copy.kdl {
        let source = expansion.repo.join("pkg").join("kdl");
        if !source.is_dir() {
            return Err(format!(
                "repository schema tree is missing at {}",
                source.display()
            ));
        }
        let target = ws.join("kdl");
        if target.symlink_metadata().is_ok() {
            return Err(format!(
                "{} already exists in the workspace; the template must not carry the schema tree",
                target.display()
            ));
        }
        std::fs::create_dir(&target)
            .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
        scratch::copy_tree(&source, &target)
            .map_err(|e| format!("cannot copy the schema tree into the workspace: {e}"))?;
    }
    let program = PathBuf::from(&command[0]);
    let output = spawn(&program, &command[1..], cwd, env, expansion)
        .map_err(|e| format!("cannot run {}: {e}", command[0]))?;
    require_exit(&output, &command[0])
}

/// Run the declared command and judge its exit and stream substrings,
/// as a positive control does.
fn exercise(
    command: &[String],
    stdout_contains: &[String],
    stdout_not_contains: &[String],
    stderr_contains: &[String],
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> Result<(), String> {
    let program = PathBuf::from(&command[0]);
    let output = spawn(&program, &command[1..], cwd, env, expansion)
        .map_err(|e| format!("cannot run {}: {e}", command[0]))?;
    require_exit(&output, &command[0])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for needle in stdout_contains {
        if !stdout.contains(needle.as_str()) {
            return Err(format!(
                "stdout does not contain {needle:?}\n--- stdout ---\n{}",
                cap(&stdout)
            ));
        }
    }
    for needle in stdout_not_contains {
        if stdout.contains(needle.as_str()) {
            return Err(format!(
                "stdout contains forbidden {needle:?}\n--- stdout ---\n{}",
                cap(&stdout)
            ));
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    for needle in stderr_contains {
        if !stderr.contains(needle.as_str()) {
            return Err(format!(
                "stderr does not contain {needle:?}\n--- stderr ---\n{}",
                cap(&stderr)
            ));
        }
    }
    Ok(())
}

/// Run one command with the harness environment and no stdin, both
/// streams captured. Cargo builds into the run's own scratch root, so
/// concurrent suite runs never share a target directory.
fn spawn(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &RunEnv<'_>,
    expansion: &Expansion<'_>,
) -> std::io::Result<Output> {
    let mut path = OsString::from(env.bin_dir.as_os_str());
    if let Some(inherited) = std::env::var_os("PATH") {
        path.push(":");
        path.push(inherited);
    }
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("PATH", path)
        .env(ENV_SUITE, env.suite)
        .env(ENV_KDLITH, env.kdlith)
        .env(ENV_SCRATCH, env.scratch)
        .env(ENV_SCENARIO, cwd)
        .env(ENV_REPO, expansion.repo)
        .env("CARGO_TARGET_DIR", scratch::cargo_target(env.scratch))
        .stdin(Stdio::null())
        .output()
}

/// A zero exit, or the command's streams as the failure report.
fn require_exit(output: &Output, what: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{what} exited {} --- stdout ---\n{}\n--- stderr ---\n{}",
        output
            .status
            .code()
            .map_or_else(|| "by signal".to_owned(), |c| c.to_string()),
        cap(&String::from_utf8_lossy(&output.stdout)),
        cap(&String::from_utf8_lossy(&output.stderr)),
    ))
}

/// Every file under `dir`, keyed by its path relative to `dir`, with
/// its bytes. A missing directory or any link or special file is an
/// error; the walk never follows one.
fn tree_files(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    collect_files(dir, "", &mut files)?;
    Ok(files)
}

fn collect_files(
    dir: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
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
            collect_files(&item.path(), &key, out)?;
        } else if file_type.is_file() {
            let bytes = std::fs::read(item.path())
                .map_err(|e| format!("cannot read {}: {e}", item.path().display()))?;
            out.insert(key, bytes);
        } else {
            return Err(format!(
                "{} holds a link or special file; only plain files and directories compare",
                item.path().display()
            ));
        }
    }
    Ok(())
}

/// The Rust source files of a tree, keyed like [`tree_files`]. Any
/// non-Rust file the tree holds is ignored, so the comparison covers
/// exactly `src/**/*.rs`.
fn rust_files(dir: &Path, what: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let all = tree_files(dir).map_err(|e| format!("{what}: {e}"))?;
    if all.is_empty() {
        return Err(format!(
            "{what}: no files to compare under {}",
            dir.display()
        ));
    }
    Ok(all
        .into_iter()
        .filter(|(path, _)| path.ends_with(".rs"))
        .collect())
}

/// The first byte position two slices disagree at.
fn first_diff(one: &[u8], other: &[u8]) -> usize {
    one.iter()
        .zip(other)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| one.len().min(other.len()))
}

/// A stream capped for a failure report. The cut lands on a character
/// boundary so the truncation itself cannot panic.
fn cap(text: &str) -> String {
    if text.len() <= STREAM_CAP {
        return text.to_owned();
    }
    let mut end = STREAM_CAP;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &text[..end], text.len())
}
