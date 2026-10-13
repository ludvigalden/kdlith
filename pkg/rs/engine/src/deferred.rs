//! Spellings of mechanisms the dialect may declare deferred. A construct
//! spelled this way is rejected as relying on the deferred mechanism,
//! never silently ignored or read as something else.

use crate::definition::Deferral;

/// How a deferred mechanism shows up in a definition document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spelling<'s> {
    Node(&'s str),
    Property(&'s str),
}

/// Node and property names that belong to deferred mechanisms, from the
/// design drafts the dialect's deferral notes cite.
const SPELLINGS: &[(&str, Spelling<'static>)] = &[
    ("codecs", Spelling::Node("codec")),
    ("codecs", Spelling::Property("codec")),
    ("union-dispatch", Spelling::Property("prefix")),
    ("union-dispatch", Spelling::Property("case")),
    ("sensitivity", Spelling::Property("sensitivity")),
    ("expansion", Spelling::Node("include")),
    ("expansion", Spelling::Node("expand")),
    ("dependent-binding", Spelling::Node("against")),
    ("case-dispatch", Spelling::Node("case")),
    ("case-dispatch", Spelling::Node("otherwise")),
    ("source-context", Spelling::Node("source-context")),
    ("source-context", Spelling::Node("path-stem")),
    ("accumulating-context", Spelling::Node("accumulate")),
    ("context-snapshots", Spelling::Node("snapshot")),
    ("profiles", Spelling::Node("profile")),
    ("descendant-rules", Spelling::Node("descendants")),
    ("ordered-sequences", Spelling::Node("sequence")),
    ("ordered-sequences", Spelling::Node("alternatives")),
    ("discovery-grammars", Spelling::Node("discovery")),
    ("discovery-grammars", Spelling::Node("anchor")),
    ("import-aliases", Spelling::Property("as")),
    ("symbol-catalogs", Spelling::Node("identity-domain")),
    ("symbol-catalogs", Spelling::Node("lookup")),
    ("contribution-resolution", Spelling::Property("merge")),
    ("contribution-resolution", Spelling::Node("resolution")),
    ("computed-defaults", Spelling::Property("computed")),
    ("secret-providers", Spelling::Node("secret")),
    ("secret-providers", Spelling::Node("provider")),
];

/// The mechanism a spelling relies on, when the governing dialect marks
/// that mechanism deferred.
pub(crate) fn deferred_by<'d>(
    deferrals: &'d [Deferral],
    spelling: Spelling<'_>,
) -> Option<&'d str> {
    SPELLINGS
        .iter()
        .filter(|(_, known)| *known == spelling)
        .find_map(|(mechanism, _)| {
            deferrals
                .iter()
                .find(|deferral| deferral.mechanism == *mechanism)
                .map(|deferral| deferral.mechanism.as_str())
        })
}

/// A deferral by mechanism name.
pub(crate) fn defers(deferrals: &[Deferral], mechanism: &str) -> bool {
    deferrals
        .iter()
        .any(|deferral| deferral.mechanism == mechanism)
}

/// Whether an import path uses discovery syntax rather than a literal
/// path: globs, brace alternatives, or an anchor.
pub(crate) fn is_discovery_path(path: &str) -> bool {
    path.starts_with('$') || path.contains(['*', '?', '{', '[', '}'])
}
