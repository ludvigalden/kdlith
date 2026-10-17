//! The built-in context handlers the data verbs install through
//! `--handler`: generic computations a schema may name in an effects
//! group. Generated processors install their own; this catalog gives
//! the CLI a real registry surface.

use std::path::PathBuf;
use std::sync::Arc;

use kdlith_engine::{HandlerCall, HandlerEffect, HandlerFn, Handlers, StringFormat};
use kdlith_model::Value;

/// `copy-fields` sets each context slot the node's field names, from
/// that field's value. A field naming no declared slot fails closed at
/// the effect point (`KDLITH-CTX005`).
const COPY_FIELDS: &str = "copy-fields";

/// `clear-namespace` clears the context namespace each of the node's
/// string fields names. A non-string field names no namespace and
/// produces no effect; an undeclared namespace fails closed at the
/// effect point (`KDLITH-CTX005`).
const CLEAR_NAMESPACE: &str = "clear-namespace";

/// `remove-fields` unsets each context slot the node's field names,
/// whatever the field's value. A field naming no declared slot fails
/// closed at the effect point (`KDLITH-CTX005`).
const REMOVE_FIELDS: &str = "remove-fields";

/// The names of the built-in catalog, in name order.
pub(crate) const BUILT_INS: &[&str] = &[CLEAR_NAMESPACE, COPY_FIELDS, REMOVE_FIELDS];

/// One built-in handler's implementation, or `None` for a name the
/// catalog does not carry.
fn built_in(name: &str) -> Option<HandlerFn> {
    match name {
        CLEAR_NAMESPACE => Some(Arc::new(clear_namespace)),
        COPY_FIELDS => Some(Arc::new(copy_fields)),
        REMOVE_FIELDS => Some(Arc::new(remove_fields)),
        _ => None,
    }
}

fn copy_fields(call: &HandlerCall<'_>) -> Vec<HandlerEffect> {
    call.fields
        .iter()
        .map(|(name, value)| HandlerEffect::Set {
            slot: (*name).clone(),
            value: value.clone(),
        })
        .collect()
}

fn remove_fields(call: &HandlerCall<'_>) -> Vec<HandlerEffect> {
    call.fields
        .keys()
        .map(|name| HandlerEffect::Remove { slot: name.clone() })
        .collect()
}

fn clear_namespace(call: &HandlerCall<'_>) -> Vec<HandlerEffect> {
    call.fields
        .values()
        .filter_map(|value| match value {
            Value::String(namespace) => Some(HandlerEffect::Clear {
                namespace: namespace.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The registry the named built-ins install. Every name was validated
/// against the catalog at argument parse time, so an unknown name never
/// reaches this constructor.
pub(crate) fn registry(names: &[String]) -> Handlers {
    let mut handlers = Handlers::new();
    for name in names {
        let handler = built_in(name).expect("argument parsing admits only catalog names");
        handlers.install(name.clone(), handler);
    }
    handlers
}

/// Validate one `--anchor` value: `NAME=PATH` with a non-empty kebab
/// name and a non-empty path.
pub(crate) fn parse_anchor(value: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = value
        .split_once('=')
        .ok_or_else(|| format!("anchor takes NAME=PATH, not {value:?}"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "anchor name {name:?} is not a non-empty kebab name"
        ));
    }
    if path.is_empty() {
        return Err(format!("anchor {name:?} takes a non-empty path"));
    }
    Ok((name.to_owned(), PathBuf::from(path)))
}

/// A `--profiles` value: the listed profile names, in listed order.
#[derive(Clone, Debug)]
pub(crate) struct ProfileSet(pub(crate) Vec<String>);

/// Validate one `--profiles` value: a comma-separated list of non-empty
/// kebab names, the `kebab-case` form every declared profile name
/// takes. Whether the
/// schema declares each name is decided at reading time
/// (`KDLITH-SCH011`), never here.
pub(crate) fn parse_profiles(value: &str) -> Result<ProfileSet, String> {
    let mut names = Vec::new();
    for name in value.split(',') {
        if name.is_empty() {
            return Err(format!(
                "profiles takes NAME[,NAME...] with no empty name, not {value:?}"
            ));
        }
        if !StringFormat::KebabCase.matches(name, None) {
            return Err(format!("profile name {name:?} is not a kebab name"));
        }
        names.push(name.to_owned());
    }
    Ok(ProfileSet(names))
}

/// Validate one `--handler` value against the built-in catalog.
pub(crate) fn parse_handler(value: &str) -> Result<String, String> {
    if BUILT_INS.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(format!(
            "no built-in handler {value:?}; the catalog carries {}",
            BUILT_INS.join(", ")
        ))
    }
}
