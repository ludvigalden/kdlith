//! The export delivery surface every processor shares: NDJSON encoding
//! of bound items and the staging writer behind atomic and provisional
//! delivery. Write failures are `KDLITH-EXP001`, never short writes.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_diagnostic::registry::{lookup, render};
use serde::Serialize;

const UNWRITABLE: &str = "KDLITH-EXP001";

/// When the destination sees results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delivery {
    /// Stage every line — in memory for stdout, in a part file beside a
    /// file destination — and publish only after processing succeeded,
    /// by one write or one rename. A failing run leaves the destination
    /// exactly as it was.
    Atomic,
    /// Write each completed line through as it is selected. A later
    /// processing failure leaves the partial stream in place; the driver
    /// names it incomplete with `KDLITH-EXP002`.
    Provisional,
}

/// One NDJSON line per item under the pinned writer profile: the item's
/// serialized form in field order, closed by a newline. Identical items
/// produce identical bytes.
pub fn write_ndjson(out: &mut impl Write, item: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, item)?;
    out.write_all(b"\n")
}

/// A `KDLITH-EXP001` record from the real write path.
fn unwritable(path: &str, reason: &str) -> Box<DiagnosticRecord> {
    let record = DiagnosticRecord::from_code(UNWRITABLE)
        .expect("KDLITH-EXP001 is registered and never retired");
    let record = match lookup(UNWRITABLE) {
        Some(definition) => {
            record.with_message(render(definition.message_template, &[path, reason]))
        }
        None => record,
    };
    Box::new(record.with_origin("export"))
}

/// The writer behind one export destination: lines pass through the
/// delivery contract the run named.
pub struct ExportWriter {
    dest: Dest,
    /// The destination as diagnostics name it: `stdout`, or the output
    /// path as the invoker wrote it.
    label: String,
    emitted: usize,
}

enum Dest {
    /// Atomic stdout: lines staged in memory until commit.
    StagedMemory(Vec<u8>),
    /// Atomic file: a part file beside the destination, renamed onto it
    /// at commit.
    StagedFile {
        file: fs::File,
        part: PathBuf,
        dest: PathBuf,
    },
    /// Provisional stdout.
    Stdout(io::Stdout),
    /// Provisional file.
    File(fs::File),
}

impl ExportWriter {
    /// Open the destination under the delivery contract: `None` is
    /// stdout, `Some(path)` a file. An unopenable destination is
    /// `KDLITH-EXP001`.
    pub fn open(delivery: Delivery, output: Option<&Path>) -> Result<Self, Box<DiagnosticRecord>> {
        let Some(path) = output else {
            return Ok(match delivery {
                Delivery::Atomic => Self {
                    dest: Dest::StagedMemory(Vec::new()),
                    label: "stdout".to_owned(),
                    emitted: 0,
                },
                Delivery::Provisional => Self {
                    dest: Dest::Stdout(io::stdout()),
                    label: "stdout".to_owned(),
                    emitted: 0,
                },
            });
        };
        let label = path.display().to_string();
        let dest = match delivery {
            Delivery::Provisional => match fs::File::create(path) {
                Ok(file) => Dest::File(file),
                Err(error) => return Err(unwritable(&label, &error.to_string())),
            },
            Delivery::Atomic => {
                let part = part_path(path);
                match fs::File::create(&part) {
                    Ok(file) => Dest::StagedFile {
                        file,
                        part,
                        dest: path.to_owned(),
                    },
                    Err(error) => return Err(unwritable(&label, &error.to_string())),
                }
            }
        };
        Ok(Self {
            dest,
            label,
            emitted: 0,
        })
    }

    /// Write one item as an NDJSON line; under provisional delivery the
    /// line is visible at the destination once this returns. A failed
    /// write is `KDLITH-EXP001` naming the destination.
    pub fn write_item(&mut self, item: &impl Serialize) -> Result<(), Box<DiagnosticRecord>> {
        let written = match &mut self.dest {
            Dest::StagedMemory(buffer) => write_ndjson(buffer, item),
            Dest::StagedFile { file, .. } => write_ndjson(&mut *file, item),
            Dest::Stdout(out) => write_ndjson(&mut *out, item).and_then(|()| out.flush()),
            Dest::File(file) => write_ndjson(&mut *file, item).and_then(|()| file.flush()),
        };
        match written {
            Ok(()) => {
                self.emitted += 1;
                Ok(())
            }
            Err(error) => Err(unwritable(&self.label, &error.to_string())),
        }
    }

    /// How many items were written.
    pub fn emitted(&self) -> usize {
        self.emitted
    }

    /// Publish the stream. Atomic delivery writes the staged lines to
    /// stdout in one write, or renames the part file onto the
    /// destination in one step; provisional delivery flushes its
    /// already-visible stream. A failed publication is `KDLITH-EXP001`.
    pub fn commit(self) -> Result<(), Box<DiagnosticRecord>> {
        match self.dest {
            Dest::StagedMemory(buffer) => {
                let mut out = io::stdout().lock();
                out.write_all(&buffer)
                    .and_then(|()| out.flush())
                    .map_err(|error| unwritable("stdout", &error.to_string()))
            }
            Dest::StagedFile {
                file: staged,
                part,
                dest,
            } => {
                let label = dest.display().to_string();
                staged
                    .sync_all()
                    .map_err(|error| unwritable(&label, &error.to_string()))?;
                drop(staged);
                fs::rename(&part, &dest).map_err(|error| unwritable(&label, &error.to_string()))
            }
            Dest::Stdout(mut out) => out
                .flush()
                .map_err(|error| unwritable("stdout", &error.to_string())),
            Dest::File(mut file) => file
                .flush()
                .map_err(|error| unwritable(&self.label, &error.to_string())),
        }
    }

    /// Drop the staged stream without publishing: a failed atomic run
    /// leaves the destination as it was and removes the part file.
    pub fn discard(self) {
        if let Dest::StagedFile { file: _, part, .. } = self.dest {
            let _ = fs::remove_file(part);
        }
    }
}

/// The part file an atomic file delivery stages into: a hidden sibling
/// of the destination, so the final rename never crosses a filesystem.
fn part_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".to_owned());
    dest.with_file_name(format!(".{name}.kdlith-export-part"))
}
