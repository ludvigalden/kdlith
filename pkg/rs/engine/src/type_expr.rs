//! The `type-expression` validator, version 0: a type-description
//! grammar inside a quoted string, compiled once to a [`TypeExpr`].

use crate::names::Names;
use crate::types::TypeExpr;

/// Constructors nest at most this deep; deeper input fails closed.
const MAX_DEPTH: usize = 32;

/// Why a type expression did not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExprError {
    /// Not in the grammar; the reason is user-facing.
    Syntax(String),
}

/// Parse `name`, `constructor<arg, ...>`, and the generic
/// instantiation `T<A, B>`, interning every type name.
pub(crate) fn parse(text: &str, names: &mut Names) -> Result<TypeExpr, ExprError> {
    let mut parser = ExprParser { text, at: 0 };
    let expr = parser.expr(names, 0)?;
    parser.skip_space();
    if parser.at < text.len() {
        return Err(ExprError::Syntax(format!(
            "unexpected text after the type at byte {}",
            parser.at
        )));
    }
    Ok(expr)
}

struct ExprParser<'t> {
    text: &'t str,
    at: usize,
}

impl<'t> ExprParser<'t> {
    fn expr(&mut self, names: &mut Names, depth: usize) -> Result<TypeExpr, ExprError> {
        if depth > MAX_DEPTH {
            return Err(ExprError::Syntax(format!(
                "nesting deeper than {MAX_DEPTH}"
            )));
        }
        self.skip_space();
        let head = self.ident()?;
        self.skip_space();
        let args = if self.eat('<') {
            let mut args = vec![self.expr(names, depth + 1)?];
            loop {
                self.skip_space();
                if self.eat(',') {
                    args.push(self.expr(names, depth + 1)?);
                } else if self.eat('>') {
                    break;
                } else {
                    return Err(ExprError::Syntax(format!(
                        "expected ',' or '>' at byte {}",
                        self.at
                    )));
                }
            }
            Some(args)
        } else {
            None
        };
        build(head, args, names)
    }

    fn ident(&mut self) -> Result<&'t str, ExprError> {
        let rest = &self.text[self.at..];
        let len = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
            .count();
        if len == 0 {
            return Err(ExprError::Syntax(format!(
                "expected a type name at byte {}",
                self.at
            )));
        }
        let start = self.at;
        self.at += len;
        Ok(&self.text[start..self.at])
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.text[self.at..].starts_with(ch) {
            self.at += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start_matches(' ').len();
    }
}

fn build(
    head: &str,
    args: Option<Vec<TypeExpr>>,
    names: &mut Names,
) -> Result<TypeExpr, ExprError> {
    let arity = match head {
        "optional" | "nullable" | "list" | "ref" => Some(1),
        "map" | "ordered-map" => Some(2),
        "union" => None,
        _ => {
            if !is_type_name(head) {
                return Err(ExprError::Syntax(format!(
                    "{head} is neither a constructor nor a type name"
                )));
            }
            return Ok(match args {
                None => TypeExpr::Named(names.intern(head)),
                // A declared type instantiated with type arguments.
                Some(args) => TypeExpr::Instantiation(names.intern(head), args),
            });
        }
    };
    let Some(mut args) = args else {
        return Err(ExprError::Syntax(format!("{head} needs type arguments")));
    };
    let fits = match arity {
        Some(count) => args.len() == count,
        None => args.len() >= 2,
    };
    if !fits {
        let wanted = arity.map_or_else(|| "at least 2".to_owned(), |count| count.to_string());
        return Err(ExprError::Syntax(format!(
            "{head} takes {wanted} type arguments, found {}",
            args.len()
        )));
    }
    let mut next = || Box::new(args.remove(0));
    Ok(match head {
        "optional" => TypeExpr::Optional(next()),
        "nullable" => TypeExpr::Nullable(next()),
        "list" => TypeExpr::List(next()),
        "ref" => TypeExpr::Ref(next()),
        "map" => TypeExpr::Map(next(), next()),
        "ordered-map" => TypeExpr::OrderedMap(next(), next()),
        _ => TypeExpr::Union(args),
    })
}

/// `^[A-Z][A-Za-z0-9]*$`, the dialect's `TypeName`.
pub(crate) fn is_type_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_uppercase()) && bytes.all(|b| b.is_ascii_alphanumeric())
}
