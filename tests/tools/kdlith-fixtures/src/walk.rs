//! The no-follow scratch walk: every path resolves component by
//! component from a root descriptor with `openat`/`fstatat`/
//! `readlinkat`; a symlink is an entry to classify, never a step.

use std::collections::BTreeMap;
use std::path::Path;

use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, RawDir, openat, readlinkat, statat};
use rustix::io::Errno;

/// The kind of an on-disk entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Dir,
    Symlink,
    /// Socket, FIFO or device: never read, never an expected entry.
    Special,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::File => "file",
            Kind::Dir => "dir",
            Kind::Symlink => "symlink",
            Kind::Special => "special",
        }
    }

    fn from_file_type(t: FileType) -> Self {
        match t {
            FileType::RegularFile => Kind::File,
            FileType::Directory => Kind::Dir,
            FileType::Symlink => Kind::Symlink,
            _ => Kind::Special,
        }
    }
}

/// What the walk found at a path.
#[derive(Clone, Debug)]
pub(crate) enum Verdict {
    Present(EntryInfo),
    Missing,
    TraversesSymlink { at: String },
    NotADirectory { at: String },
    Unreadable,
    Error(String),
}

/// Everything known about one on-disk entry.
#[derive(Clone, Debug)]
pub(crate) struct EntryInfo {
    pub(crate) kind: Kind,
    /// Permission bits (`& 0o7777`).
    pub(crate) mode: u32,
    pub(crate) size: u64,
    /// Symlink target text for links.
    pub(crate) link_target: Option<Vec<u8>>,
    /// Content for regular files, read after an fstat re-check.
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) mtime: (i64, u32),
}

/// A validated `/`-separated path relative to a scratch root: no
/// empty, `.`, `..`, or NUL component, so a walk stays inside the
/// root by construction.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RelPath(String);

impl RelPath {
    /// Validate and adopt `rel` as a root-relative path.
    pub(crate) fn new(rel: &str) -> Result<Self, String> {
        if rel.is_empty() {
            return Err("relative path is empty".to_string());
        }
        for comp in rel.split('/') {
            if comp.is_empty() {
                return Err(format!("relative path {rel:?} has an empty component"));
            }
            if comp == "." || comp == ".." {
                return Err(format!("relative path {rel:?} has a non-normal component"));
            }
            if comp.contains('\0') {
                return Err(format!("relative path {rel:?} has a NUL component"));
            }
        }
        Ok(Self(rel.to_string()))
    }

    /// The path text (empty for the root).
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The display form (`.` for the root).
    pub(crate) fn display(&self) -> &str {
        if self.0.is_empty() { "." } else { &self.0 }
    }

    fn components(&self) -> Vec<&str> {
        if self.0.is_empty() {
            Vec::new()
        } else {
            self.0.split('/').collect()
        }
    }

    fn join(&self, name: &str) -> Self {
        let joined = if self.0.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", self.0, name)
        };
        Self(joined)
    }
}

const DIR_OPEN: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// Open a scratch root directory. The caller owns the descriptor;
/// every walk below starts from it.
pub(crate) fn open_root(path: &Path) -> std::io::Result<OwnedFd> {
    openat(rustix::fs::CWD, path, DIR_OPEN, Mode::empty()).map_err(std::io::Error::from)
}

/// Walk `rel` from the root descriptor, then examine the final
/// component. Never follows a link, in the final component or in
/// any ancestor.
pub(crate) fn walk(root_fd: &OwnedFd, rel: &RelPath) -> Verdict {
    let comps = rel.components();
    let mut owned: Option<OwnedFd> = None;
    for comp in &comps[..comps.len().saturating_sub(1)] {
        let dirfd: &OwnedFd = owned.as_ref().unwrap_or(root_fd);
        match openat(dirfd, *comp, DIR_OPEN, Mode::empty()) {
            Ok(next) => owned = Some(next),
            Err(Errno::NOENT) => return Verdict::Missing,
            Err(Errno::LOOP) => return classify_ancestor(root_fd, &comps, comp),
            Err(Errno::NOTDIR) => return classify_ancestor(root_fd, &comps, comp),
            Err(Errno::ACCESS) => return Verdict::Unreadable,
            Err(other) => return Verdict::Error(other.to_string()),
        }
    }
    let dirfd: &OwnedFd = owned.as_ref().unwrap_or(root_fd);
    let last = comps.last().copied().unwrap_or(".");
    match statat(dirfd, last, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) => info_from_stat(dirfd, last, &st),
        Err(Errno::NOENT) => Verdict::Missing,
        Err(Errno::NOTDIR) => Verdict::NotADirectory {
            at: last.to_string(),
        },
        Err(Errno::ACCESS) => Verdict::Unreadable,
        Err(other) => Verdict::Error(other.to_string()),
    }
}

/// An ancestor open failed with ELOOP or ENOTDIR: classify the
/// component itself with `fstatat(AT_SYMLINK_NOFOLLOW)` — a link is
/// `traverses-symlink`, anything else `not-a-directory`.
fn classify_ancestor(root_fd: &OwnedFd, comps: &[&str], comp: &str) -> Verdict {
    let idx = comps.iter().position(|c| *c == comp).unwrap_or(0);
    let mut owned: Option<OwnedFd> = None;
    for c in &comps[..idx] {
        let dirfd: &OwnedFd = owned.as_ref().unwrap_or(root_fd);
        match openat(dirfd, *c, DIR_OPEN, Mode::empty()) {
            Ok(next) => owned = Some(next),
            Err(_) => break,
        }
    }
    let dirfd: &OwnedFd = owned.as_ref().unwrap_or(root_fd);
    match statat(dirfd, comp, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Symlink => {
            Verdict::TraversesSymlink {
                at: comp.to_string(),
            }
        }
        Ok(_) => Verdict::NotADirectory {
            at: comp.to_string(),
        },
        Err(Errno::NOENT) => Verdict::Missing,
        Err(Errno::ACCESS) => Verdict::Unreadable,
        Err(other) => Verdict::Error(other.to_string()),
    }
}

fn info_from_stat(dirfd: &OwnedFd, name: &str, st: &rustix::fs::Stat) -> Verdict {
    let kind = Kind::from_file_type(FileType::from_raw_mode(st.st_mode));
    let base = EntryInfo {
        kind,
        mode: Mode::from_raw_mode(st.st_mode).as_raw_mode(),
        size: st.st_size as u64,
        link_target: None,
        bytes: None,
        mtime: (st.st_mtime, st.st_mtime_nsec as u32),
    };
    match kind {
        Kind::Symlink => match readlinkat(dirfd, name, Vec::new()) {
            Ok(target) => Verdict::Present(EntryInfo {
                link_target: Some(target.as_bytes().to_vec()),
                ..base
            }),
            Err(e) => Verdict::Error(e.to_string()),
        },
        Kind::File => match read_file_checked(dirfd, name, st) {
            Ok(bytes) => Verdict::Present(EntryInfo {
                bytes: Some(bytes),
                ..base
            }),
            Err(v) => v,
        },
        Kind::Dir | Kind::Special => Verdict::Present(base),
    }
}

/// Read a regular file through a descriptor, re-checking type and
/// identity after the open so a swapped-in FIFO never blocks the
/// read.
fn read_file_checked(
    dirfd: &OwnedFd,
    name: &str,
    expect: &rustix::fs::Stat,
) -> Result<Vec<u8>, Verdict> {
    let open = OFlags::RDONLY
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC);
    let fd = match openat(dirfd, name, open, Mode::empty()) {
        Ok(f) => f,
        Err(Errno::LOOP) => {
            return Err(Verdict::TraversesSymlink {
                at: name.to_string(),
            });
        }
        Err(Errno::NOENT) => return Err(Verdict::Missing),
        Err(Errno::ACCESS) => return Err(Verdict::Unreadable),
        Err(other) => return Err(Verdict::Error(other.to_string())),
    };
    let st = match rustix::fs::fstat(&fd) {
        Ok(s) => s,
        Err(e) => return Err(Verdict::Error(e.to_string())),
    };
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || st.st_dev != expect.st_dev
        || st.st_ino != expect.st_ino
    {
        return Err(Verdict::Error(
            "entry changed between stat and open".to_string(),
        ));
    }
    let mut file = std::fs::File::from(fd);
    let mut buf = Vec::new();
    use std::io::Read;
    match file.read_to_end(&mut buf) {
        Ok(_) => Ok(buf),
        Err(e) => Err(Verdict::Error(e.to_string())),
    }
}

/// One entry of a captured tree, keyed relative to the scratch root
/// (the root itself has the empty relative path).
#[derive(Clone, Debug)]
pub(crate) struct TreeEntry {
    pub(crate) rel: String,
    pub(crate) kind: Kind,
    pub(crate) mode: u32,
    pub(crate) size: u64,
    pub(crate) link_target: Option<Vec<u8>>,
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) mtime: (i64, u32),
}

#[derive(Debug)]
pub(crate) enum TreeError {
    TraversesSymlink { at: String },
    NotADirectory { at: String },
    Unreadable,
    Error(String),
}

impl TreeError {
    /// The failure text for reports.
    pub(crate) fn text(&self) -> String {
        match self {
            TreeError::TraversesSymlink { at } => format!("traverses-symlink at {at:?}"),
            TreeError::NotADirectory { at } => format!("not-a-directory at {at:?}"),
            TreeError::Unreadable => "unreadable".to_string(),
            TreeError::Error(e) => format!("walk error: {e}"),
        }
    }
}

/// Capture the complete tree at `rel` (a file, a link or a
/// directory); an absent path captures as the empty tree. Symlinks
/// are entries, never descended.
pub(crate) fn capture_tree(root_fd: &OwnedFd, rel: &RelPath) -> Result<Vec<TreeEntry>, TreeError> {
    let info = match walk(root_fd, rel) {
        Verdict::Present(info) => info,
        Verdict::Missing => return Ok(Vec::new()),
        Verdict::TraversesSymlink { at } => return Err(TreeError::TraversesSymlink { at }),
        Verdict::NotADirectory { at } => return Err(TreeError::NotADirectory { at }),
        Verdict::Unreadable => return Err(TreeError::Unreadable),
        Verdict::Error(e) => return Err(TreeError::Error(e)),
    };
    let mut out = vec![tree_entry(rel.as_str(), &info)];
    if info.kind == Kind::Dir {
        let dirfd = open_path_dir(root_fd, rel)?;
        descend(&dirfd, rel, &mut out)?;
    }
    Ok(out)
}

fn tree_entry(rel: &str, info: &EntryInfo) -> TreeEntry {
    TreeEntry {
        rel: rel.to_string(),
        kind: info.kind,
        mode: info.mode,
        size: info.size,
        link_target: info.link_target.clone(),
        bytes: info.bytes.clone(),
        mtime: info.mtime,
    }
}

/// Enumerate an open directory and recurse below it; every child is
/// examined relative to the descriptor, never re-walked by name from
/// the root.
fn descend(dirfd: &OwnedFd, prefix: &RelPath, out: &mut Vec<TreeEntry>) -> Result<(), TreeError> {
    for name in list_dir(dirfd)? {
        let st = match statat(dirfd, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(s) => s,
            Err(Errno::NOENT) => {
                return Err(TreeError::Error("entry vanished mid-walk".into()));
            }
            Err(Errno::ACCESS) => return Err(TreeError::Unreadable),
            Err(e) => return Err(TreeError::Error(e.to_string())),
        };
        let info = match info_from_stat(dirfd, name.as_str(), &st) {
            Verdict::Present(i) => i,
            Verdict::Missing => {
                return Err(TreeError::Error("entry vanished mid-walk".into()));
            }
            Verdict::Unreadable => return Err(TreeError::Unreadable),
            Verdict::Error(e) => return Err(TreeError::Error(e)),
            Verdict::TraversesSymlink { .. } | Verdict::NotADirectory { .. } => {
                return Err(TreeError::Error("walk inconsistent".into()));
            }
        };
        let child = prefix.join(&name);
        let is_dir = info.kind == Kind::Dir;
        out.push(tree_entry(child.as_str(), &info));
        if is_dir {
            let cfd = openat(dirfd, name.as_str(), DIR_OPEN, Mode::empty())
                .map_err(|e| TreeError::Error(e.to_string()))?;
            descend(&cfd, &child, out)?;
        }
    }
    Ok(())
}

/// Open the directory descriptor for `rel` by the no-follow
/// component walk from the root descriptor.
fn open_path_dir(root_fd: &OwnedFd, rel: &RelPath) -> Result<OwnedFd, TreeError> {
    let mut current: Option<OwnedFd> = None;
    for comp in rel.components() {
        let dirfd: &OwnedFd = current.as_ref().unwrap_or(root_fd);
        match openat(dirfd, comp, DIR_OPEN, Mode::empty()) {
            Ok(next) => current = Some(next),
            Err(Errno::ACCESS) => return Err(TreeError::Unreadable),
            Err(e) => return Err(TreeError::Error(e.to_string())),
        }
    }
    current.ok_or_else(|| TreeError::Error("empty path".into()))
}

/// List one directory's entry names, sorted, without following
/// anything; types come from the per-entry `statat` in the caller.
fn list_dir(dirfd: &OwnedFd) -> Result<Vec<String>, TreeError> {
    let mut buf: Vec<std::mem::MaybeUninit<u8>> = vec![std::mem::MaybeUninit::new(0); 64 * 1024];
    let mut raw = RawDir::new(dirfd.as_fd(), &mut buf);
    let mut out = Vec::new();
    loop {
        match raw.next() {
            Some(Ok(entry)) => {
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                match std::str::from_utf8(name) {
                    Ok(s) => out.push(s.to_string()),
                    Err(_) => {
                        return Err(TreeError::Error("non-UTF-8 entry name".into()));
                    }
                }
            }
            Some(Err(e)) => return Err(TreeError::Error(e.to_string())),
            None => break,
        }
    }
    out.sort();
    Ok(out)
}

/// What one declared scratch entry must look like.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Expected {
    /// The required entry kind.
    pub(crate) kind: Kind,
    /// Required permission bits (`& 0o777`) when declared.
    pub(crate) mode: Option<u32>,
}

/// Classify the observed tree at `rel` against `expected`, whose
/// keys are relative to `rel`: declared entries must exist with the
/// declared kind and mode, and every observed entry outside the
/// declaration is a foreign leftover. Returns every violation in
/// sorted order, never only the first.
pub(crate) fn classify(
    root_fd: &OwnedFd,
    rel: &RelPath,
    expected: &BTreeMap<String, Expected>,
) -> Vec<String> {
    let tree = match capture_tree(root_fd, rel) {
        Ok(t) => t,
        Err(e) => return vec![format!("cannot walk {}: {}", rel.display(), e.text())],
    };
    let root_str = rel.as_str();
    let prefix = if root_str.is_empty() {
        String::new()
    } else {
        format!("{root_str}/")
    };
    let observed: BTreeMap<&str, &TreeEntry> = tree
        .iter()
        .filter_map(|e| {
            if e.rel.is_empty() || e.rel == root_str {
                return None;
            }
            e.rel.strip_prefix(prefix.as_str()).map(|c| (c, e))
        })
        .collect();
    let mut out = Vec::new();
    for (decl, exp) in expected {
        match observed.get(decl.as_str()) {
            None => out.push(format!("declared entry '{decl}' is missing")),
            Some(entry) => {
                if entry.kind != exp.kind {
                    out.push(format!(
                        "'{decl}' kind {} does not equal {}",
                        entry.kind.as_str(),
                        exp.kind.as_str()
                    ));
                }
                if let Some(mode) = exp.mode {
                    if entry.mode & 0o777 != mode {
                        out.push(format!(
                            "'{decl}' mode {:04o} does not equal {:04o}",
                            entry.mode & 0o777,
                            mode
                        ));
                    }
                }
            }
        }
    }
    for (obs, entry) in &observed {
        if !expected.contains_key(*obs) {
            out.push(format!("foreign entry '{obs}' ({})", entry.kind.as_str()));
        }
    }
    out
}
