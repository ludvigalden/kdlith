//! Content digests: SHA-256 of a file's bytes, and the canonical digest
//! of a tree of files, the identity a source is pinned by. The
//! canonical form is defined with the locations reference.

use std::path::Path;

use sha2::{Digest, Sha256};

/// The digest of `bytes`, written `sha256:<hex>`.
pub(crate) fn bytes_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

/// The canonical digest of the files `files` lists, each a portable
/// path relative to the tree's root with its bytes, in any order.
pub(crate) fn files_digest<'f, P, B>(
    files: impl IntoIterator<Item = (P, B)>,
) -> Result<String, String>
where
    P: AsRef<str> + 'f,
    B: AsRef<[u8]> + 'f,
{
    let mut lines: Vec<(String, String)> = Vec::new();
    for (path, bytes) in files {
        let path = path.as_ref();
        if path.contains(['\n', '\\']) {
            return Err(format!("the path {path:?} has no canonical line"));
        }
        lines.push((path.to_owned(), hex(&Sha256::digest(bytes.as_ref()))));
    }
    Ok(lines_digest(lines))
}

/// The canonical digest of files already hashed: each a portable path
/// with the hex SHA-256 of its bytes, in any order.
pub(crate) fn lines_digest(mut lines: Vec<(String, String)>) -> String {
    lines.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut hasher = Sha256::new();
    for (path, digest) in &lines {
        hasher.update(digest.as_bytes());
        hasher.update(b"  ");
        hasher.update(path.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{}", hex(&hasher.finalize()))
}

/// The hex SHA-256 of `bytes`, as a canonical digest line spells it.
pub(crate) fn bytes_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The source-set fingerprint: the SHA-256 of one `NAME IDENTITY` line
/// per named source a reading used, in name order; an identity a tree
/// cannot have is written `none`.
pub(crate) fn set_fingerprint<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut hasher = Sha256::new();
    for (name, identity) in pairs {
        hasher.update(name.as_bytes());
        hasher.update(b" ");
        hasher.update(identity.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{}", hex(&hasher.finalize()))
}

/// The canonical digest of the directory tree at `root`.
pub fn tree_digest(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let read = std::fs::read_dir(&dir).map_err(|error| error.to_string())?;
        for entry in read {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .ok()
                .and_then(|relative| relative.to_str())
                .map(|relative| relative.replace(std::path::MAIN_SEPARATOR, "/"))
                .ok_or_else(|| format!("the path {} is not UTF-8", path.display()))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
                files.push((relative, bytes));
            } else if kind.is_symlink() {
                return Err(format!("the tree holds a link at {relative}"));
            } else {
                return Err(format!("the tree holds a special file at {relative}"));
            }
        }
    }
    files_digest(files)
}

/// Whether `digest` is a SHA-256 digest as a pin writes one.
pub(crate) fn is_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The lowercase hex of a SHA-256 digest, as a store path names it.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}
