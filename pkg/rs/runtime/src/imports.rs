//! The import invocation stack: literal paths relative to the importer,
//! projection-only contexts so mutations never escape, cycles found on
//! the active stack, and repeats handled by policy (reject by default).

use std::path::{Path, PathBuf};

pub use kdlith_model::Repeated;
use kdlith_model::Value;

use crate::context::{Context, ContextError, SlotId, stable_hash, stable_hash_step};

/// Imports nest at most this deep; deeper chains fail closed.
pub const MAX_IMPORT_DEPTH: usize = 64;

/// One session invokes at most this many sources by default: entry
/// documents and import instantiations alike. A host raises or lowers
/// the bound with [`Invocations::with_limit`].
pub const MAX_INVOCATIONS: usize = 1024;

/// One invocation of a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct InvocationId(u32);

impl InvocationId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The semantics one invocation binds under: which revision of its
/// source, which schema, which inherited context, which profile
/// selection, which values a registered value resolver supplied, under
/// which repeat policy. Two invocations with one identity produce one
/// bound result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct InvocationIdentity {
    /// The source revision: a digest of the document's text.
    pub revision: u64,
    /// The schema fingerprint the invocation binds under.
    pub schema: u64,
    /// A fingerprint of the inherited (projected) context.
    pub context: u64,
    /// A fingerprint of the resolved profile selection the invocation
    /// binds under: a result bound under one selection is never the
    /// result of another.
    pub profiles: u64,
    /// A fingerprint of the non-secret values the value resolver
    /// supplied while binding (see [`ResolvedStream`]); the empty
    /// stream's fingerprint when it supplied none.
    pub resolved: u64,
    /// The repeat policy the request carried.
    pub policy: Repeated,
}

impl InvocationIdentity {
    pub fn new(
        revision: u64,
        schema: u64,
        context: u64,
        profiles: u64,
        resolved: u64,
        policy: Repeated,
    ) -> Self {
        Self {
            revision,
            schema,
            context,
            profiles,
            resolved,
            policy,
        }
    }

    /// The same identity with the resolved-values component a binding
    /// produced: known only once the invocation has bound.
    #[must_use]
    pub fn with_resolved(mut self, resolved: u64) -> Self {
        self.resolved = resolved;
        self
    }
}

/// The resolved-values component of an invocation identity: a
/// domain-separated fingerprint of the length-prefixed stream of each
/// resolution's site, field, and value, in resolution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedStream(u64);

impl Default for ResolvedStream {
    fn default() -> Self {
        Self(stable_hash(b"kdlith-resolved"))
    }
}

impl ResolvedStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one resolution in: the node site's byte span (positional
    /// within the document, so equal under every invocation of it), the
    /// field name, and the value.
    pub fn push(&mut self, (start, end): (u32, u32), field: &str, value: &Value) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&end.to_le_bytes());
        prefixed(&mut bytes, field.as_bytes());
        let mut encoded = Vec::new();
        encode_value(&mut encoded, value);
        prefixed(&mut bytes, &encoded);
        let mut record = Vec::with_capacity(bytes.len() + 8);
        prefixed(&mut record, &bytes);
        self.0 = stable_hash_step(self.0, &record);
    }

    pub fn fingerprint(self) -> u64 {
        self.0
    }
}

/// Append `bytes` preceded by their length.
fn prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// A canonical, unambiguous encoding of a value: a tag byte, then
/// length-prefixed parts. A reference's source site is positional, not
/// semantic, so it takes no part.
fn encode_value(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => out.push(0),
        Value::Bool(held) => out.extend_from_slice(&[1, u8::from(*held)]),
        Value::Number(text) => {
            out.push(2);
            prefixed(out, text.as_bytes());
        }
        Value::String(text) => {
            out.push(3);
            prefixed(out, text.as_bytes());
        }
        Value::List(items) => {
            out.push(4);
            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
            for item in items {
                encode_value(out, item);
            }
        }
        Value::Map(entries) => {
            out.push(5);
            out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
            for entry in entries {
                prefixed(out, entry.key.as_bytes());
                encode_value(out, &entry.value);
            }
        }
        Value::Reference(reference) => {
            out.push(6);
            prefixed(out, reference.domain.as_bytes());
            out.extend_from_slice(&(reference.name.0.len() as u64).to_le_bytes());
            for segment in &reference.name.0 {
                prefixed(out, segment.as_bytes());
            }
            out.extend_from_slice(&(reference.qualifiers.len() as u64).to_le_bytes());
            for (key, held) in &reference.qualifiers {
                prefixed(out, key.as_bytes());
                prefixed(out, held.as_bytes());
            }
            prefixed(out, reference.written.as_bytes());
        }
        // A value variant this version does not know still encodes
        // deterministically within the running build, so the
        // fingerprint never silently ignores a resolved value.
        other => {
            out.push(0xff);
            prefixed(out, format!("{other:?}").as_bytes());
        }
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
    /// The repeat policy the request carried; entry documents reject
    /// repeats by construction.
    pub policy: Repeated,
    /// The schema fingerprint the session binds under.
    pub schema: u64,
    /// A fingerprint of the context as the invocation was entered with.
    pub context_fingerprint: u64,
    /// The fingerprint of the profile selection the session binds the
    /// invocation under.
    pub profiles: u64,
    /// The source revision, once the host has read the document's text;
    /// `None` until then.
    pub revision: Option<u64>,
}

impl Invocation {
    /// The invocation's identity: complete once its revision is known.
    /// Its resolved-values component is the empty stream's until a
    /// binding that consulted a value resolver supplies its own.
    pub fn identity(&self) -> Option<InvocationIdentity> {
        Some(InvocationIdentity {
            revision: self.revision?,
            schema: self.schema,
            context: self.context_fingerprint,
            profiles: self.profiles,
            resolved: ResolvedStream::new().fingerprint(),
            policy: self.policy,
        })
    }
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
#[derive(Debug)]
pub struct Invocations {
    invocations: Vec<Invocation>,
    active: Vec<InvocationId>,
    /// The most invocations the session admits, entries included.
    limit: usize,
    /// The schema fingerprint every invocation of the session binds
    /// under; recorded on each invocation as it is admitted.
    schema: u64,
    /// The fingerprint of the profile selection the invocations admitted
    /// from now on bind under; recorded on each as it is admitted.
    profiles: u64,
}

impl Default for Invocations {
    fn default() -> Self {
        Self {
            invocations: Vec::new(),
            active: Vec::new(),
            limit: MAX_INVOCATIONS,
            schema: 0,
            profiles: 0,
        }
    }
}

impl Invocations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit at most `limit` invocations in the session — entry
    /// documents and import instantiations together — in place of
    /// [`MAX_INVOCATIONS`]. The invocation past the bound fails closed
    /// with [`ImportFailure::Limit`].
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Record the schema fingerprint every invocation of the session
    /// binds under; part of every invocation's identity.
    pub fn with_schema_fingerprint(mut self, schema: u64) -> Self {
        for invocation in &mut self.invocations {
            invocation.schema = schema;
        }
        self.schema = schema;
        self
    }

    /// Record the fingerprint of the profile selection every invocation
    /// admitted from now on binds under — an entry document and the
    /// documents its imports reach; part of each one's identity.
    pub fn select_profiles(&mut self, fingerprint: u64) {
        self.profiles = fingerprint;
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
        self.push(canonical, shown, context, None, Repeated::Reject)
    }

    /// Invoke an entry document held in memory, shown as `shown`: the
    /// same registration [`Invocations::enter_root`] gives a file
    /// entry, with its shown spelling standing in for a canonical
    /// path. A document held in memory resolves no imports of its own;
    /// the reading fails them closed.
    pub fn enter_root_in_memory(
        &mut self,
        shown: &str,
        context: Context,
    ) -> Result<InvocationId, ImportFailure> {
        let path = PathBuf::from(shown);
        if let Some(first) = self.find(&path) {
            return Err(ImportFailure::Repeated {
                target: shown.to_owned(),
                first,
            });
        }
        self.push(path, shown.to_owned(), context, None, Repeated::Reject)
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
            return Err(no_importer(target));
        };
        let context = self.invocations[importer.index()]
            .context
            .project(project)
            .map_err(ImportFailure::Projection)?;
        self.request_projected(target, repeated, context)
    }

    /// Resolve a request whose projection the caller made from its live
    /// context, as a streaming session does at the import site.
    pub fn request_projected(
        &mut self,
        target: &str,
        repeated: Repeated,
        context: Context,
    ) -> Result<Admission, ImportFailure> {
        let Some(&importer) = self.active.last() else {
            return Err(no_importer(target));
        };
        let caller = &self.invocations[importer.index()];
        let base = caller.path.parent().unwrap_or(Path::new(""));
        let shown_base = Path::new(&caller.shown).parent().unwrap_or(Path::new(""));
        let shown = shown_base.join(target).display().to_string();
        let resolved = canonical(&base.join(target), &shown)?;
        self.admit(resolved, shown, repeated, context)
    }

    /// Admit a target the caller already resolved, such as a pattern
    /// match: the same cycle, limit, and repeat checks a written request
    /// passes, against a path the caller expanded itself.
    pub fn invoke_resolved(
        &mut self,
        resolved: &Path,
        shown: &str,
        repeated: Repeated,
        context: Context,
    ) -> Result<Admission, ImportFailure> {
        if self.active.is_empty() {
            return Err(no_importer(shown));
        }
        self.admit(resolved.to_owned(), shown.to_owned(), repeated, context)
    }

    /// The checks every admitted target passes, and the push.
    fn admit(
        &mut self,
        resolved: PathBuf,
        shown: String,
        repeated: Repeated,
        context: Context,
    ) -> Result<Admission, ImportFailure> {
        let Some(&importer) = self.active.last() else {
            return Err(no_importer(&shown));
        };
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
                Repeated::Coalesce if same(&self.invocations[first.index()].context, &context) => {
                    return Ok(Admission::Reuse(first));
                }
                Repeated::Coalesce | Repeated::Instantiate => {}
                // Reject, and any policy this stack does not know,
                // fails closed.
                _ => {
                    return Err(ImportFailure::Repeated {
                        target: shown,
                        first,
                    });
                }
            }
        }
        self.push(resolved, shown, context, Some(importer), repeated)
            .map(Admission::Invoke)
    }

    /// Leave the innermost active invocation.
    pub fn leave(&mut self) {
        self.active.pop();
    }

    /// The innermost active invocation: the importer of any request this
    /// stack is currently executing.
    pub fn innermost(&self) -> Option<InvocationId> {
        self.active.last().copied()
    }

    pub fn get(&self, id: InvocationId) -> Option<&Invocation> {
        self.invocations.get(id.index())
    }

    /// Record the source revision of an invocation, once its document's
    /// text has been read. `false` when the invocation is unknown.
    pub fn supply_revision(&mut self, id: InvocationId, revision: u64) -> bool {
        self.invocations
            .get_mut(id.index())
            .map(|invocation| invocation.revision = Some(revision))
            .is_some()
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
        policy: Repeated,
    ) -> Result<InvocationId, ImportFailure> {
        if self.invocations.len() >= self.limit {
            return Err(ImportFailure::Limit {
                limit: "invocation count (entries plus imports)",
                max: self.limit,
            });
        }
        let context_fingerprint = context.fingerprint();
        let schema = self.schema;
        let profiles = self.profiles;
        let id = index(self.invocations.len());
        self.invocations.push(Invocation {
            path,
            shown,
            context,
            caller,
            policy,
            schema,
            context_fingerprint,
            profiles,
            revision: None,
        });
        self.active.push(id);
        Ok(id)
    }
}

fn no_importer(target: &str) -> ImportFailure {
    ImportFailure::Unresolved {
        target: target.to_owned(),
        reason: "no importing document is active".to_owned(),
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
