//! Name and type-expression rendering: how a compiled definition's
//! spellings become Rust identifiers, Rust type spellings, and the
//! pinned layout's include paths. Every function is pure.

use kdlith_engine::{Names, TypeExpr};

use crate::Error;

/// A kebab-case or already-Pascal spelling, as a Rust type or variant
/// name: each dash-separated word capitalized, words joined.
pub fn pascal(name: &str) -> String {
    let mut spelled = String::with_capacity(name.len());
    for word in name.split('-') {
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            spelled.extend(first.to_uppercase());
            spelled.push_str(characters.as_str());
        }
    }
    spelled
}

/// A field key as a Rust field name: dashes to underscores, `type`
/// spelled `ty` as the declaration tables do, other Rust keywords as raw
/// identifiers.
pub fn field_name(key: &str) -> String {
    if key == "type" {
        return "ty".to_owned();
    }
    let snake = key.replace('-', "_");
    if is_keyword(&snake) {
        format!("r#{snake}")
    } else {
        snake
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "yield"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "gen"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
    )
}

/// Whether `word` is a plain ASCII Rust identifier: a letter or
/// underscore, then letters, digits, or underscores, and not `_` alone.
pub fn is_identifier(word: &str) -> bool {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    word != "_" && characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every named type a type expression references, spelled as declared.
pub fn named_references(expr: &TypeExpr, names: &Names) -> Vec<String> {
    match expr {
        TypeExpr::Named(sym) => vec![names.resolve(*sym).to_owned()],
        TypeExpr::Instantiation(_, arguments) | TypeExpr::Union(arguments) => arguments
            .iter()
            .flat_map(|inner| named_references(inner, names))
            .collect(),
        TypeExpr::Optional(inner)
        | TypeExpr::List(inner)
        | TypeExpr::Nullable(inner)
        | TypeExpr::Ref(inner) => named_references(inner, names),
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            let mut found = named_references(key, names);
            found.extend(named_references(value, names));
            found
        }
        // A constructor this version does not know names nothing the
        // tables can spell.
        _ => Vec::new(),
    }
}

/// The Rust type a type expression renders to, or the reason it cannot:
/// exactly the set the engine's decoding supports, everything else
/// fails closed. An inline `union<…>` renders as `union`, the enum its
/// owner's field declares; where no field owns it, it fails closed.
pub fn rust_type(expr: &TypeExpr, names: &Names, union: Option<&str>) -> Result<String, Error> {
    match expr {
        TypeExpr::Named(sym) => {
            let name = names.resolve(*sym);
            Ok(match name {
                "String" => "String".to_owned(),
                "Bool" => "bool".to_owned(),
                "U64" => "u64".to_owned(),
                "Integer" => "i64".to_owned(),
                "Decimal" => "Decimal".to_owned(),
                "ValueExpr" => "ValueExpr".to_owned(),
                declared => declared.to_owned(),
            })
        }
        TypeExpr::Optional(inner) => Ok(format!("Option<{}>", rust_type(inner, names, union)?)),
        TypeExpr::Nullable(inner) => Ok(format!("Nullable<{}>", rust_type(inner, names, union)?)),
        TypeExpr::List(inner) => Ok(format!("Vec<{}>", rust_type(inner, names, union)?)),
        TypeExpr::OrderedMap(key, value) => Ok(format!(
            "OrderedMap<{}, {}>",
            rust_type(key, names, union)?,
            rust_type(value, names, union)?
        )),
        TypeExpr::Map(key, value) => Ok(format!(
            "BTreeMap<{}, {}>",
            rust_type(key, names, union)?,
            rust_type(value, names, union)?
        )),
        // A reference reads as the engine's symbolic `Ref`, resolved
        // against the reading's declarations by linking, never by
        // reading; linking admits only a declared type name inside it.
        TypeExpr::Ref(inner) => match inner.as_ref() {
            TypeExpr::Named(_) => Ok(format!("Ref<{}>", rust_type(inner, names, None)?)),
            _ => Err(Error::UnsupportedExpression {
                spelling: expression_text(expr, names),
            }),
        },
        TypeExpr::Union(_) => match union {
            Some(name) => Ok(name.to_owned()),
            None => Err(Error::UnsupportedExpression {
                spelling: expression_text(expr, names),
            }),
        },
        // A generic instantiation surviving to generation, or a
        // constructor this version does not know, cannot decode; it
        // fails closed rather than emitting a type that cannot read.
        _ => Err(Error::UnsupportedExpression {
            spelling: expression_text(expr, names),
        }),
    }
}

/// The name of the enum an inline union in `owner`'s field `key`
/// renders as.
pub fn inline_union_name(owner: &str, key: &str) -> String {
    format!("{owner}{}", pascal(key))
}

/// The variant an inline union member reads as: the words of its Rust
/// rendering, each capitalized, joined — `u64` as `U64`, `Vec<String>`
/// as `VecString`.
pub fn variant_name(rendering: &str) -> String {
    rendering
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .flat_map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase())
                .into_iter()
                .chain(characters)
        })
        .collect()
}

/// Every sub-expression of a type expression, itself first, in
/// pre-order.
pub fn subexpressions(expr: &TypeExpr) -> Vec<&TypeExpr> {
    let mut found = vec![expr];
    match expr {
        TypeExpr::Optional(inner)
        | TypeExpr::Nullable(inner)
        | TypeExpr::List(inner)
        | TypeExpr::Ref(inner) => found.extend(subexpressions(inner)),
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            found.extend(subexpressions(key));
            found.extend(subexpressions(value));
        }
        TypeExpr::Instantiation(_, arguments) | TypeExpr::Union(arguments) => {
            for argument in arguments {
                found.extend(subexpressions(argument));
            }
        }
        _ => {}
    }
    found
}

/// Whether a type expression's rendering spells the standard library's
/// `BTreeMap`, which the file must import.
pub fn spells_btree_map(expr: &TypeExpr) -> bool {
    subexpressions(expr)
        .into_iter()
        .any(|sub| matches!(sub, TypeExpr::Map(_, _)))
}

/// The engine names a type expression's Rust rendering spells, beyond
/// the named types it references: the generic carriers the engine
/// defines, at any depth.
pub fn engine_names(expr: &TypeExpr, names: &Names) -> Vec<&'static str> {
    let mut found = Vec::new();
    collect_engine_names(expr, names, &mut found);
    found
}

fn collect_engine_names(expr: &TypeExpr, names: &Names, found: &mut Vec<&'static str>) {
    let push = |name: &'static str, found: &mut Vec<&'static str>| {
        if !found.contains(&name) {
            found.push(name);
        }
    };
    match expr {
        TypeExpr::Named(sym) => match names.resolve(*sym) {
            "ValueExpr" => push("ValueExpr", found),
            "Decimal" => push("Decimal", found),
            _ => {}
        },
        TypeExpr::Ref(inner) => {
            push("Ref", found);
            collect_engine_names(inner, names, found);
        }
        TypeExpr::OrderedMap(key, value) => {
            push("OrderedMap", found);
            collect_engine_names(key, names, found);
            collect_engine_names(value, names, found);
        }
        TypeExpr::Nullable(inner) => {
            push("Nullable", found);
            collect_engine_names(inner, names, found);
        }
        TypeExpr::Optional(inner) | TypeExpr::List(inner) => {
            collect_engine_names(inner, names, found);
        }
        TypeExpr::Map(key, value) => {
            collect_engine_names(key, names, found);
            collect_engine_names(value, names, found);
        }
        TypeExpr::Instantiation(_, arguments) | TypeExpr::Union(arguments) => {
            for argument in arguments {
                collect_engine_names(argument, names, found);
            }
        }
        _ => {}
    }
}

/// The definition's own spelling of a type expression, for error text.
pub fn expression_text(expr: &TypeExpr, names: &Names) -> String {
    match expr {
        TypeExpr::Named(sym) => names.resolve(*sym).to_owned(),
        TypeExpr::Instantiation(head, arguments) => format!(
            "{}<{}>",
            names.resolve(*head),
            arguments
                .iter()
                .map(|argument| expression_text(argument, names))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Optional(inner) => format!("optional<{}>", expression_text(inner, names)),
        TypeExpr::Nullable(inner) => format!("nullable<{}>", expression_text(inner, names)),
        TypeExpr::List(inner) => format!("list<{}>", expression_text(inner, names)),
        TypeExpr::Map(key, value) => format!(
            "map<{}, {}>",
            expression_text(key, names),
            expression_text(value, names)
        ),
        TypeExpr::OrderedMap(key, value) => format!(
            "ordered-map<{}, {}>",
            expression_text(key, names),
            expression_text(value, names)
        ),
        TypeExpr::Union(members) => format!(
            "union<{}>",
            members
                .iter()
                .map(|member| expression_text(member, names))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Ref(inner) => format!("ref<{}>", expression_text(inner, names)),
        // A constructor this version does not know still spells in the
        // definition's own grammar, which is all error text needs.
        other => format!("{other:?}"),
    }
}

/// Lexically normalize a document path: resolve `.` and `..` segments
/// without touching the filesystem, so the shown paths a reader reports
/// become the repository-relative paths generation spells.
pub fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else if !absolute {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The include path a seed document takes from the generated crate's
/// `src/` directory, under the pinned layout: the crate sits at
/// `pkg/rs/<name>`, so every seed document must sit somewhere under the
/// repository's `pkg/` tree.
pub fn include_path(path: &str) -> Result<String, Error> {
    let normalized = normalize_path(path);
    let segments: Vec<&str> = normalized.split('/').collect();
    let Some(at) = segments.iter().position(|segment| *segment == "pkg") else {
        return Err(Error::SeedOutsidePkg { path: normalized });
    };
    let under_pkg = segments[at + 1..].join("/");
    Ok(format!("../../../{under_pkg}"))
}
