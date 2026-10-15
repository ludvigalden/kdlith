//! The built-in context handlers the data verbs install through
//! `--handler`: generic computations a schema may name in an effects
//! group. Generated processors install their own; this catalog gives
//! the CLI a real registry surface.

use std::path::PathBuf;
use std::sync::Arc;

use kdlith_engine::{HandlerCall, HandlerEffect, HandlerFn, Handlers};

/// `copy-fields` sets each context slot the node's field names, from
/// that field's value. A field naming no declared slot fails closed at
/// the effect point (`KDLITH-CTX005`).
const COPY_FIELDS: &str = "copy-fields";

/// The names of the built-in catalog, in name order.
pub(crate) const BUILT_INS: &[&str] = &[COPY_FIELDS];

/// One built-in handler's implementation, or `None` for a name the
/// catalog does not carry.
fn built_in(name: &str) -> Option<HandlerFn> {
    match name {
        COPY_FIELDS => Some(Arc::new(copy_fields)),
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
