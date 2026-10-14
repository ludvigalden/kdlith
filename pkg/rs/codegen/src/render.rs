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
    )
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
/// fails closed.
pub fn rust_type(expr: &TypeExpr, names: &Names) -> Result<String, Error> {
    match expr {
        TypeExpr::Named(sym) => {
            let name = names.resolve(*sym);
            Ok(match name {
                "String" => "String".to_owned(),
                "Bool" => "bool".to_owned(),
                "U64" => "u64".to_owned(),
                "ValueExpr" => "ValueExpr".to_owned(),
                "Integer" | "Decimal" => {
                    return Err(Error::UnsupportedExpression {
                        spelling: name.to_owned(),
                    });
                }
                declared => declared.to_owned(),
            })
        }
        TypeExpr::Optional(inner) => Ok(format!("Option<{}>", rust_type(inner, names)?)),
        TypeExpr::List(inner) => Ok(format!("Vec<{}>", rust_type(inner, names)?)),
        TypeExpr::OrderedMap(key, value) => Ok(format!(
            "OrderedMap<{}, {}>",
            rust_type(key, names)?,
            rust_type(value, names)?
        )),
        TypeExpr::Nullable(_)
        | TypeExpr::Map(_, _)
        | TypeExpr::Union(_)
        | TypeExpr::Ref(_)
        | TypeExpr::Instantiation(_, _) => Err(Error::UnsupportedExpression {
            spelling: expression_text(expr, names),
        }),
        // A constructor this version does not know cannot decode; it
        // fails closed rather than emitting a type that cannot read.
        _ => Err(Error::UnsupportedExpression {
            spelling: expression_text(expr, names),
        }),
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
