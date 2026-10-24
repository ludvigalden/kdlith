//! Where a typed item was read: its document and its import's namespace.

/// The document a typed record or declaration was read from, in one live
/// reading; not the export envelope's persisted value provenance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Origin {
    /// The document's path as the reading showed it: an entry as the
    /// invocation spelled it, an import as its request resolved it.
    pub path: String,
    /// The namespace of the namespaced import that loaded the
    /// document; `None` for an entry document or an import without one.
    pub namespace: Option<String>,
}
