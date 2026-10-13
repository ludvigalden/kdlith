//! Clean-run teardown: a bottom-up no-follow remover that deletes
//! only the harness-created run tree, removes a symlink as a link,
//! and records leftovers loudly instead of silently deleting.

use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, RawDir, openat, statat, unlinkat};

const DIR_OPEN: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// One entry observed during the bottom-up walk, with the metadata
/// needed to re-verify identity before descending.
struct Observed {
    name: String,
    kind: FileType,
    dev: u64,
    ino: u64,
    mode: u32,
}

/// Remove the directory tree at `dir` bottom-up from descriptors,
/// never following a link; a directory that lost owner `rwx` has it
/// restored first. Returns the paths that could not be removed.
pub(crate) fn remove_tree(dir: &Path) -> Vec<String> {
    let fd = match openat(rustix::fs::CWD, dir, DIR_OPEN, Mode::empty()) {
        Ok(f) => f,
        Err(_) => return vec![dir.to_string_lossy().to_string()],
    };
    let mut failures = Vec::new();
    remove_below(&fd, &mut failures);
    if std::fs::remove_dir(dir).is_err() {
        failures.push(dir.to_string_lossy().to_string());
    }
    failures
}

fn remove_below(dir_fd: &OwnedFd, failures: &mut Vec<String>) {
    for observed in list_observed(dir_fd) {
        if observed.kind == FileType::Directory {
            let _ = restore_dir_mode(dir_fd, observed.name.as_str(), observed.mode);
            let child = match openat(dir_fd, observed.name.as_str(), DIR_OPEN, Mode::empty()) {
                Ok(f) => f,
                Err(_) => {
                    failures.push(observed.name.clone());
                    continue;
                }
            };
            let same = statat(&child, ".", AtFlags::empty())
                .map(|st| st.st_dev == observed.dev && st.st_ino == observed.ino)
                .unwrap_or(false);
            if !same {
                failures.push(observed.name.clone());
                continue;
            }
            remove_below(&child, failures);
            if unlinkat(dir_fd, observed.name.as_str(), AtFlags::REMOVEDIR).is_err() {
                failures.push(observed.name.clone());
            }
        } else if unlinkat(dir_fd, observed.name.as_str(), AtFlags::empty()).is_err() {
            failures.push(observed.name.clone());
        }
    }
}

/// List one directory's entries with their no-follow metadata,
/// sorted by name.
fn list_observed(dir_fd: &OwnedFd) -> Vec<Observed> {
    let mut buf: Vec<std::mem::MaybeUninit<u8>> = vec![std::mem::MaybeUninit::new(0); 64 * 1024];
    let mut raw = RawDir::new(dir_fd.as_fd(), &mut buf);
    let mut out = Vec::new();
    while let Some(Ok(entry)) = raw.next() {
        let name = entry.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        let Ok(name) = std::str::from_utf8(name) else {
            continue;
        };
        let Ok(st) = statat(dir_fd, name, AtFlags::SYMLINK_NOFOLLOW) else {
            continue;
        };
        out.push(Observed {
            name: name.to_string(),
            kind: FileType::from_raw_mode(st.st_mode),
            dev: st.st_dev,
            ino: st.st_ino,
            mode: st.st_mode & 0o7777,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Restore owner `rwx` on a directory entry without following a
/// link: open it `O_PATH|O_NOFOLLOW`, confirm by `fstat`, and chmod
/// through `/proc/self/fd`, which resolves to the entry itself.
fn restore_dir_mode(parent: &OwnedFd, name: &str, current_mode: u32) -> std::io::Result<()> {
    if current_mode & 0o700 == 0o700 {
        return Ok(());
    }
    let want = (current_mode & !0o700) | 0o700;
    let fd = openat(
        parent,
        name,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let st = rustix::fs::fstat(&fd).map_err(std::io::Error::from)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::Directory {
        return Err(std::io::Error::other("entry changed during restore"));
    }
    let proc_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
    std::fs::set_permissions(&proc_path, std::fs::Permissions::from_mode(want))
}

/// The outcome of tearing down one run directory.
#[derive(Debug)]
pub(crate) struct TeardownReport {
    /// The run directory the teardown addressed.
    pub(crate) run: String,
    /// The paths that survived removal.
    pub(crate) leftovers: Vec<String>,
}

impl TeardownReport {
    /// Whether nothing survived the teardown.
    pub(crate) fn clean(&self) -> bool {
        self.leftovers.is_empty()
    }
}

/// Tear down the harness-owned run directory at `run_root` and
/// nothing else: sibling runs under the scratch root belong to other
/// invocations, and a link planted at or below the run root is
/// removed as a link, never traversed.
pub(crate) fn teardown_run(run_root: &Path) -> TeardownReport {
    let leftovers = remove_tree(run_root);
    TeardownReport {
        run: run_root.to_string_lossy().to_string(),
        leftovers,
    }
}

/// Print every leftover loudly so a dirty teardown is never silent.
pub(crate) fn emit_leftovers(report: &TeardownReport) {
    for path in &report.leftovers {
        eprintln!(
            "kdlith-fixtures: leftover: could not remove {} under run {}",
            path, report.run
        );
    }
}

/// Remove one materialized scenario directory with the shared
/// no-follow remover; the caller reports the leftovers.
pub(crate) fn remove_scenario_dir(dir: &Path) -> Vec<String> {
    remove_tree(dir)
}
