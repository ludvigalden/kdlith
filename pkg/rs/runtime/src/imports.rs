//! The import invocation stack: literal paths relative to the importer,
//! projection-only contexts so mutations never escape, cycles found on
//! the active stack, and repeats handled by policy (reject by default).

use std::path::{Path, PathBuf};

use crate::context::{Context, ContextError, SlotId};

/// How a source imported again in one session is handled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Repeated {
    /// A second import of the same source is an error.
    #[default]
    Reject,
    /// A second import under an equal projected context reuses the first
    /// invocation.
    Coalesce,
    /// Each import is a separate invocation.
    Instantiate,
}

/// Imports nest at most this deep; deeper chains fail closed.
pub const MAX_IMPORT_DEPTH: usize = 64;

/// One session invokes at most this many sources.
pub const MAX_INVOCATIONS: usize = 1024;

/// An invocation in its session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvocationId(u32);

impl InvocationId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One invocation of a source.
#[derive(Debug)]
#[non_exhaustive]
pub struct Invocation {
    /// The canonical path, the source's identity in the session.
    pub path: PathBuf,
    /// The path as written, joined onto its importer's shown directory.
    pub shown: String,
    /// The invocation's own context: the caller's projection at entry.
    pub context: Context,
    /// The invocation that imported it; `None` for an entry document.
    pub caller: Option<InvocationId>,
}

/// What a request resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Admission {
    /// A new invocation, now the innermost active one; leave it after
    /// its document is processed.
    Invoke(InvocationId),
    /// `repeated=coalesce` under an equal context: the first invocation
    /// is reused and nothing is processed again.
    Reuse(InvocationId),
}

/// Why a request was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportFailure {
    /// The target does not name a readable document.
    Unresolved { target: String, reason: String },
    /// The target is active on the invocation stack; `chain` runs from
    /// its active invocation to the request that closes the cycle.
    Cycle { chain: Vec<String> },
    /// The target was invoked before and the policy rejects repeats.
    Repeated { target: String, first: InvocationId },
    /// A depth or invocation-count bound would be exceeded.
    Limit { limit: &'static str, max: usize },
    /// The projection names a slot outside the caller's layout.
    Projection(ContextError),
}

/// The invocations of one session and its active stack.
#[derive(Debug, Default)]
pub struct Invocations {
    invocations: Vec<Invocation>,
    active: Vec<InvocationId>,
}

impl Invocations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invoke an entry document. Entries are never imported, so a
    /// document named twice as an entry is a rejected repeat.
    pub fn enter_root(
        &mut self,
        path: &Path,
        context: Context,
    ) -> Result<InvocationId, ImportFailure> {
        let shown = path.display().to_string();
        let canonical = canonical(path, &shown)?;
        if let Some(first) = self.find(&canonical) {
            return Err(ImportFailure::Repeated {
                target: shown,
                first,
            });
        }
        self.push(canonical, shown, context, None)
    }

    /// Resolve a request made by the innermost active invocation. The
    /// imported document gets the projected slots of its caller's
    /// context as a context of its own, so nothing it does reaches the
    /// caller.
    pub fn request(
        &mut self,
        target: &str,
        repeated: Repeated,
        project: &[SlotId],
    ) -> Result<Admission, ImportFailure> {
        let Some(&importer) = self.active.last() else {
            return Err(ImportFailure::Unresolved {
                target: target.to_owned(),
                reason: "no importing document is active".to_owned(),
            });
        };
        let caller = &self.invocations[importer.index()];
        let context = caller
            .context
            .project(project)
            .map_err(ImportFailure::Projection)?;
        let base = caller.path.parent().unwrap_or(Path::new(""));
        let shown_base = Path::new(&caller.shown).parent().unwrap_or(Path::new(""));
        let shown = shown_base.join(target).display().to_string();
        let resolved = canonical(&base.join(target), &shown)?;
        if let Some(at) = self
            .active
            .iter()
            .position(|id| self.invocations[id.index()].path == resolved)
        {
            let mut chain: Vec<String> = self.active[at..]
                .iter()
                .map(|id| self.invocations[id.index()].shown.clone())
                .collect();
            chain.push(shown);
            return Err(ImportFailure::Cycle { chain });
        }
        if self.active.len() >= MAX_IMPORT_DEPTH {
            return Err(ImportFailure::Limit {
                limit: "import depth",
                max: MAX_IMPORT_DEPTH,
            });
        }
        if let Some(first) = self.find(&resolved) {
            match repeated {
                Repeated::Reject => {
                    return Err(ImportFailure::Repeated {
                        target: shown,
                        first,
                    });
                }
                Repeated::Coalesce if same(&self.invocations[first.index()].context, &context) => {
                    return Ok(Admission::Reuse(first));
                }
                Repeated::Coalesce | Repeated::Instantiate => {}
            }
        }
        self.push(resolved, shown, context, Some(importer))
            .map(Admission::Invoke)
    }

    /// Leave the innermost active invocation.
    pub fn leave(&mut self) {
        self.active.pop();
    }

    pub fn get(&self, id: InvocationId) -> Option<&Invocation> {
        self.invocations.get(id.index())
    }

    /// The context an invocation's own effects apply to.
    pub fn context_mut(&mut self, id: InvocationId) -> Option<&mut Context> {
        self.invocations
            .get_mut(id.index())
            .map(|invocation| &mut invocation.context)
    }

    pub fn len(&self) -> usize {
        self.invocations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.invocations.is_empty()
    }

    fn find(&self, path: &Path) -> Option<InvocationId> {
        self.invocations
            .iter()
            .position(|invocation| invocation.path == path)
            .map(index)
    }

    fn push(
        &mut self,
        path: PathBuf,
        shown: String,
        context: Context,
        caller: Option<InvocationId>,
    ) -> Result<InvocationId, ImportFailure> {
        if self.invocations.len() >= MAX_INVOCATIONS {
            return Err(ImportFailure::Limit {
                limit: "import count",
                max: MAX_INVOCATIONS,
            });
        }
        let id = index(self.invocations.len());
        self.invocations.push(Invocation {
            path,
            shown,
            context,
            caller,
        });
        self.active.push(id);
        Ok(id)
    }
}

fn index(at: usize) -> InvocationId {
    InvocationId(u32::try_from(at).unwrap_or(u32::MAX))
}

fn canonical(path: &Path, shown: &str) -> Result<PathBuf, ImportFailure> {
    let resolved = std::fs::canonicalize(path).map_err(|error| ImportFailure::Unresolved {
        target: shown.to_owned(),
        reason: error.to_string(),
    })?;
    if resolved.is_file() {
        Ok(resolved)
    } else {
        Err(ImportFailure::Unresolved {
            target: shown.to_owned(),
            reason: "not a file".to_owned(),
        })
    }
}

/// Whether two contexts hold equal values in every slot.
fn same(left: &Context, right: &Context) -> bool {
    left.layout() == right.layout()
        && left
            .layout()
            .slots()
            .all(|slot| left.get(slot).map(|v| &v.value) == right.get(slot).map(|v| &v.value))
}
