//! The slice of rustfmt's default layout the generated tables, chains,
//! and constants reach: a generated file is already formatted, so
//! `--check` compares the bytes a write produces.

/// rustfmt's `max_width`.
const MAX_WIDTH: usize = 100;
/// rustfmt's `tab_spaces`.
const TAB: usize = 4;
/// rustfmt's `fn_call_width`: a call's arguments stay on one line only
/// within this width.
const FN_CALL_WIDTH: usize = 60;
/// rustfmt's `array_width`.
const ARRAY_WIDTH: usize = 60;
/// rustfmt's `chain_width`.
const CHAIN_WIDTH: usize = 60;
/// rustfmt's `short_array_element_width_threshold`.
const SHORT_ARRAY_ELEMENT: usize = 10;

/// An expression a generated file spells, in the forms rustfmt lays
/// out differently.
#[derive(Clone, Debug)]
pub(crate) enum Expr {
    /// A path or name: it never breaks and must fit.
    Path(String),
    /// A literal: it never breaks and is kept even where it overflows.
    Literal(String),
    /// A qualified path, `<T as Trait>::item`: it never breaks, and no
    /// array packs it with its siblings.
    Qualified(String),
    /// `callee::<generic>(args)`.
    Call {
        callee: String,
        generic: Option<String>,
        args: Vec<Expr>,
    },
    /// `|param| body`.
    Closure { param: String, body: Box<Expr> },
    /// `root.name(args)…`.
    Chain {
        root: Box<Expr>,
        links: Vec<(String, Vec<Expr>)>,
    },
    /// `&[items]`.
    Array(Vec<Expr>),
}

impl Expr {
    pub(crate) fn path(text: impl Into<String>) -> Self {
        Self::Path(text.into())
    }

    pub(crate) fn literal(text: impl Into<String>) -> Self {
        Self::Literal(text.into())
    }

    pub(crate) fn string(text: &str) -> Self {
        Self::Literal(format!("\"{text}\""))
    }

    pub(crate) fn call(callee: impl Into<String>, args: Vec<Expr>) -> Self {
        Self::Call {
            callee: callee.into(),
            generic: None,
            args,
        }
    }

    pub(crate) fn turbofish(callee: impl Into<String>, generic: &str, args: Vec<Expr>) -> Self {
        Self::Call {
            callee: callee.into(),
            generic: Some(generic.to_owned()),
            args,
        }
    }

    /// The expression with the method `name(args)` chained onto it.
    pub(crate) fn method(self, name: &str, args: Vec<Expr>) -> Self {
        match self {
            Self::Chain { root, mut links } => {
                links.push((name.to_owned(), args));
                Self::Chain { root, links }
            }
            root => Self::Chain {
                root: Box::new(root),
                links: vec![(name.to_owned(), args)],
            },
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Self::Path(_) | Self::Literal(_) | Self::Qualified(_) => Kind::Atom,
            Self::Call { .. } => Kind::Call,
            Self::Closure { .. } => Kind::Closure,
            Self::Chain { .. } => Kind::Chain,
            Self::Array(_) => Kind::Array,
        }
    }

    /// Whether an array may pack this item with its siblings.
    fn simple(&self) -> bool {
        matches!(self, Self::Path(_) | Self::Literal(_))
    }

    /// The expression on one line, however wide.
    fn flat(&self) -> String {
        match self {
            Self::Path(text) | Self::Literal(text) | Self::Qualified(text) => text.clone(),
            Self::Call {
                callee,
                generic,
                args,
            } => format!(
                "{callee}{}({})",
                generic
                    .as_ref()
                    .map_or(String::new(), |g| format!("::<{g}>")),
                flat_list(args)
            ),
            Self::Closure { param, body } => format!("|{param}| {}", body.flat()),
            Self::Chain { root, links } => {
                let mut text = root.flat();
                for (name, args) in links {
                    text.push_str(&format!(".{name}({})", flat_list(args)));
                }
                text
            }
            Self::Array(items) => format!("&[{}]", flat_list(items)),
        }
    }

    fn rewrite(&self, shape: Shape) -> Option<String> {
        match self {
            Self::Literal(text) => Some(text.clone()),
            Self::Path(text) | Self::Qualified(text) => {
                (width(text) <= shape.width).then(|| text.clone())
            }
            Self::Call {
                callee,
                generic,
                args,
            } => {
                let callee = match generic {
                    None => (width(callee) <= shape.width).then(|| callee.clone())?,
                    Some(generic) => {
                        let inner = shape.offset_left(width(callee))?;
                        let generic = [Expr::path(generic.clone())];
                        let list = List::new("", &generic, inner, "<", ">", MAX_WIDTH);
                        format!("{callee}::{}", list.no_overflow().rewrite()?)
                    }
                };
                List::new(&callee, args, shape, "(", ")", FN_CALL_WIDTH).rewrite()
            }
            Self::Closure { param, body } => {
                let prefix = format!("|{param}|");
                if let Some(inline) = shape.offset_left(width(&prefix) + 1)
                    && let Some(text) = body.rewrite(inline)
                    && !text.contains('\n')
                {
                    return Some(format!("{prefix} {text}"));
                }
                let inner = Shape::indented(shape.indent + TAB);
                let text = body.rewrite(inner)?;
                Some(format!(
                    "{prefix} {{\n{}{text}\n{}}}",
                    spaces(shape.indent + TAB),
                    spaces(shape.indent)
                ))
            }
            Self::Chain { root, links } => rewrite_chain(root, links, shape),
            Self::Array(items) => {
                let inner = shape.offset_left(1)?;
                let list = List::new("", items, inner, "[", "]", ARRAY_WIDTH).array();
                Some(format!("&{}", list.rewrite()?))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Atom,
    Call,
    Closure,
    Chain,
    Array,
}

/// `lhs expr;` as rustfmt lays an item's right-hand side out: on the
/// item's line, or on the next line one indent in. `lhs` ends in `=`
/// and carries no indentation of its own.
pub(crate) fn assignment(lhs: &str, expr: &Expr, indent: usize) -> String {
    let flat = || format!("{}{lhs} {};", spaces(indent), expr.flat());
    assign(lhs, expr, indent).unwrap_or_else(flat)
}

fn assign(lhs: &str, expr: &Expr, indent: usize) -> Option<String> {
    let shape = Shape::indented(indent).sub_width(1)?;
    let same = shape.offset_left(last_width(lhs) + 1).unwrap_or(Shape::new(
        indent,
        shape.offset + last_width(lhs) + 1,
        0,
    ));
    let original = expr.rewrite(same);
    let lead = format!("{}{lhs}", spaces(indent));
    if let Some(text) = &original
        && !text.contains('\n')
        && width(text) <= same.width
    {
        return Some(format!("{lead} {text};"));
    }
    let next = Shape::indented(indent + TAB).sub_width(same.rhs_overhead())?;
    let below = expr.rewrite(next);
    let on_next = |text: &str| format!("{lead}\n{}{text};", spaces(indent + TAB));
    match (original, below) {
        (Some(text), Some(new)) if !fits(&new, next) => Some(format!("{lead} {text};")),
        (Some(text), Some(new)) if prefer_next_line(&text, &new) => Some(on_next(&new)),
        (None, Some(new)) => Some(on_next(&new)),
        (Some(text), _) => Some(format!("{lead} {text};")),
        (None, None) => None,
    }
}

fn prefer_next_line(original: &str, new: &str) -> bool {
    if !new.contains('\n') || lines(original) > lines(new) + 1 {
        return true;
    }
    let first = |text: &str| text.split('\n').next().unwrap_or("").to_owned();
    let (original, new) = (first(original), first(new));
    ['(', '{', '[']
        .iter()
        .any(|c| original.ends_with(*c) && !new.ends_with(*c))
}

/// A `use path::{names};` statement, names in rustfmt's version order:
/// a single name unbraced, one line when it fits, greedily wrapped
/// lines otherwise.
pub(crate) fn use_list(prefix: &str, path: &str, names: &[String]) -> String {
    let mut names = names.to_vec();
    names.sort_by(|one, other| version_order(one, other));
    if let [only] = names.as_slice() {
        return format!("{prefix}use {path}::{only};\n");
    }
    let single = format!("{prefix}use {path}::{{{}}};", names.join(", "));
    // The widest one-line list rustfmt keeps, and the widest wrapped
    // line: both stop short of `max_width`.
    if width(&single) <= MAX_WIDTH - 2 {
        return format!("{single}\n");
    }
    let mut text = format!("{prefix}use {path}::{{\n");
    let mut buffer = String::new();
    for name in &names {
        if !buffer.is_empty() && TAB + width(&buffer) + 2 + width(name) + 1 > MAX_WIDTH - 1 {
            text.push_str(&format!("    {buffer},\n"));
            buffer.clear();
        }
        if !buffer.is_empty() {
            buffer.push_str(", ");
        }
        buffer.push_str(name);
    }
    text.push_str(&format!("    {buffer},\n}};\n"));
    text
}

/// rustfmt's version sort for import and module names: digit runs
/// compare by value (a longer run of equal value first), `_` before
/// every other character, everything else by code point.
pub(crate) fn version_order(one: &str, other: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b): (Vec<char>, Vec<char>) = (one.chars().collect(), other.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let run = |chars: &[char], from: usize| {
                let end = chars[from..]
                    .iter()
                    .position(|c| !c.is_ascii_digit())
                    .map_or(chars.len(), |at| from + at);
                let digits: String = chars[from..end].iter().collect();
                let value = digits.trim_start_matches('0').to_owned();
                (end, value)
            };
            let ((end_a, value_a), (end_b, value_b)) = (run(&a, i), run(&b, j));
            let by_value = value_a
                .len()
                .cmp(&value_b.len())
                .then_with(|| value_a.cmp(&value_b));
            if by_value != Ordering::Equal {
                return by_value;
            }
            let (len_a, len_b) = (end_a - i, end_b - j);
            if len_a != len_b {
                return len_b.cmp(&len_a);
            }
            (i, j) = (end_a, end_b);
            continue;
        }
        let key = |c: char| {
            if c == '_' {
                -1
            } else {
                i64::from(u32::from(c))
            }
        };
        match key(a[i]).cmp(&key(b[j])) {
            Ordering::Equal => {}
            unequal => return unequal,
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// The space a rewrite may take: its line's indentation, the columns
/// already used on its first line beyond that, and the width left.
#[derive(Clone, Copy, Debug)]
struct Shape {
    indent: usize,
    offset: usize,
    width: usize,
}

impl Shape {
    const fn new(indent: usize, offset: usize, width: usize) -> Self {
        Self {
            indent,
            offset,
            width,
        }
    }

    fn indented(indent: usize) -> Self {
        Self::new(indent, 0, MAX_WIDTH.saturating_sub(indent))
    }

    fn offset_left(self, by: usize) -> Option<Self> {
        let width = self.width.checked_sub(by)?;
        Some(Self::new(self.indent, self.offset + by, width))
    }

    fn sub_width(self, by: usize) -> Option<Self> {
        let width = self.width.checked_sub(by)?;
        Some(Self::new(self.indent, self.offset, width))
    }

    fn used(self) -> usize {
        self.indent + self.offset
    }

    fn rhs_overhead(self) -> usize {
        MAX_WIDTH.saturating_sub(self.used() + self.width)
    }
}

fn rewrite_chain(root: &Expr, links: &[(String, Vec<Expr>)], shape: Shape) -> Option<String> {
    let link = |at: usize, shape: Shape| {
        let (name, args) = &links[at];
        List::new(&format!(".{name}"), args, shape, "(", ")", FN_CALL_WIDTH).rewrite()
    };
    let mut head = root.rewrite(shape)?;
    let mut next = 0;
    // A root no wider than an indent takes its first links along.
    let tab_width = TAB.saturating_sub(shape.offset);
    while next < links.len() && width(&head) <= tab_width && !head.contains('\n') {
        let Some(text) = shape.offset_left(width(&head)).and_then(|s| link(next, s)) else {
            break;
        };
        head.push_str(&text);
        next += 1;
    }
    if next == links.len() {
        return fits(&head, shape).then_some(head);
    }
    let ends_block = matches!(root.kind(), Kind::Call | Kind::Chain) && head.contains('\n');
    let child_indent = if ends_block {
        shape.indent
    } else {
        shape.indent + TAB
    };
    let child = Shape::indented(child_indent);
    let mut rewrites = vec![head];
    for at in next..links.len() - 1 {
        rewrites.push(link(at, child)?);
    }
    let last = links.len() - 1;
    let extendable = last_line_extendable(&rewrites[0]);
    let almost_total = if extendable {
        last_width(&rewrites[0])
    } else {
        rewrites.iter().map(|text| width(text)).sum()
    };
    let limit = if links.len() - next == 1 {
        shape.width
    } else {
        shape.width.min(CHAIN_WIDTH)
    };
    let one_line = rewrites.iter().all(|text| !text.contains('\n')) && limit > almost_total;
    let budget = limit.saturating_sub(almost_total);
    let last_shape = if one_line {
        Some(shape)
    } else if extendable {
        Some(child)
    } else {
        child.sub_width(shape.rhs_overhead())
    };
    let mut on_line = false;
    let mut last_text = None;
    if (one_line || extendable)
        && let Some(rest) = last_shape.and_then(|s| s.offset_left(almost_total))
        && let Some(text) = link(last, rest)
    {
        let line_count = lines(&text) + 1;
        let could = first_width(&text) <= budget;
        if could && line_count >= 5 {
            on_line = one_line;
            last_text = Some(text);
        } else {
            let below = child
                .sub_width(shape.rhs_overhead())
                .and_then(|s| link(last, s));
            match below {
                Some(new) if !could => last_text = Some(new),
                Some(new) if lines(&new) + 1 >= line_count => {
                    on_line = could && one_line;
                    last_text = Some(text);
                }
                Some(new) => last_text = Some(new),
                None => {
                    on_line = could && one_line;
                    last_text = Some(text);
                }
            }
        }
    }
    let last_text = match last_text {
        Some(text) => text,
        None => link(last, last_shape?)?,
    };
    rewrites.push(last_text);
    let joint = if on_line {
        String::new()
    } else {
        format!("\n{}", spaces(child_indent))
    };
    let text = rewrites.join(&joint);
    fits(&text, shape).then_some(text)
}

/// A delimited list: a call's or method's arguments, an array's items,
/// a turbofish's types.
struct List<'a> {
    ident: &'a str,
    items: &'a [Expr],
    shape: Shape,
    open: &'a str,
    close: &'a str,
    item_max: usize,
    array: bool,
    overflow: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tactic {
    Horizontal,
    Vertical,
    Mixed,
}

impl<'a> List<'a> {
    fn new(
        ident: &'a str,
        items: &'a [Expr],
        shape: Shape,
        open: &'a str,
        close: &'a str,
        item_max: usize,
    ) -> Self {
        Self {
            ident,
            items,
            shape,
            open,
            close,
            item_max,
            array: false,
            overflow: true,
        }
    }

    fn array(mut self) -> Self {
        self.array = true;
        self
    }

    fn no_overflow(mut self) -> Self {
        self.overflow = false;
        self
    }

    fn rewrite(&self) -> Option<String> {
        let shape = self.shape;
        let used = if self.ident.contains('\n') {
            last_width(self.ident).saturating_sub(shape.used())
        } else {
            width(self.ident)
        };
        let one_line_width = shape.width.saturating_sub(used + 2);
        let ident_width = last_width(self.ident);
        let overflow_shape = shape
            .offset_left(ident_width + 1)
            .and_then(|s| s.sub_width(1))
            .unwrap_or(Shape::new(shape.indent, shape.offset, 0));
        let nested = Shape::new(
            shape.indent + TAB,
            0,
            MAX_WIDTH.saturating_sub(shape.indent + TAB + 1),
        );
        let (items, tactic) = self.items_text(one_line_width, overflow_shape, nested)?;
        let room = shape.width.saturating_sub(ident_width);
        let extent = if items.is_empty() {
            2
        } else {
            first_width(&items) + 1
        };
        let (ident, open, close) = (self.ident, self.open, self.close);
        if tactic == Tactic::Horizontal && extent <= room {
            return Some(format!("{ident}{open}{items}{close}"));
        }
        let closing = format!("\n{}{close}", spaces(shape.indent));
        if items.is_empty() {
            return Some(format!("{ident}{open}{closing}"));
        }
        Some(format!(
            "{ident}{open}\n{}{items}{closing}",
            spaces(nested.indent)
        ))
    }

    fn items_text(
        &self,
        one_line_width: usize,
        overflow_shape: Shape,
        nested: Shape,
    ) -> Option<(String, Tactic)> {
        let items = self.items;
        let Some((last, leading)) = items.split_last() else {
            return Some((String::new(), Tactic::Horizontal));
        };
        let count = items.len();
        let mut texts: Vec<Option<String>> = leading.iter().map(|i| i.rewrite(nested)).collect();
        let combine = count == 1 && self.overflow && width(self.ident) < TAB;
        let overflow_last = self.overflow && (combine || can_overflow(last, count));
        let mut placeholder = None;
        texts.push(None);
        if overflow_last {
            let last_shape = if count == 1 && last.kind() != Kind::Call {
                Some(overflow_shape)
            } else {
                let before: usize = texts[..count - 1]
                    .iter()
                    .map(|t| 2 + t.as_deref().map_or(0, width))
                    .sum();
                Shape::new(
                    overflow_shape.indent,
                    overflow_shape.offset,
                    self.item_max.min(overflow_shape.width),
                )
                .offset_left(before)
            };
            if let Some(text) = last_shape.and_then(|s| last.rewrite(s)) {
                texts[count - 1] = Some(first_line(&text).to_owned());
                placeholder = Some(text);
            }
        }
        let horizontal = |texts: &[Option<String>]| {
            let all = texts.iter().all(Option::is_some);
            let multi = texts.iter().flatten().any(|t| t.contains('\n'));
            let total: usize =
                texts.iter().flatten().map(|t| width(t)).sum::<usize>() + 2 * (count - 1);
            all && !multi && total <= one_line_width.min(self.item_max)
        };
        let mut tactic = if horizontal(&texts) {
            Tactic::Horizontal
        } else {
            Tactic::Vertical
        };
        match placeholder {
            Some(text) if overflow_last && tactic == Tactic::Horizontal && count == 1 => {
                let replace = if lines(&text) == 1 {
                    last.rewrite(nested).filter(|t| !t.contains('\n'))
                } else {
                    None
                };
                texts[count - 1] = Some(replace.unwrap_or(text));
            }
            Some(text) if overflow_last && tactic == Tactic::Horizontal => {
                texts[count - 1] = Some(text);
            }
            _ => {
                texts[count - 1] = last.rewrite(nested);
                let texts: Vec<&String> = texts.iter().flatten().collect();
                if texts.len() < count {
                    return None;
                }
                if count == 1
                    && one_line_width != 0
                    && !texts[0].contains('\n')
                    && width(texts[0]) <= one_line_width
                {
                    tactic = Tactic::Horizontal;
                } else {
                    let multi = texts.iter().any(|t| t.contains('\n'));
                    let total: usize =
                        texts.iter().map(|t| width(t)).sum::<usize>() + 2 * (count - 1);
                    tactic = if !multi && total <= one_line_width.min(self.item_max) {
                        Tactic::Horizontal
                    } else {
                        Tactic::Vertical
                    };
                    if tactic == Tactic::Vertical
                        && self.array
                        && items.iter().all(Expr::simple)
                        && texts.iter().all(|t| width(t) <= SHORT_ARRAY_ELEMENT)
                    {
                        tactic = Tactic::Mixed;
                    }
                }
            }
        }
        let texts: Vec<String> = texts.into_iter().collect::<Option<_>>()?;
        let indent = spaces(nested.indent);
        let text = match tactic {
            Tactic::Horizontal => texts.join(", "),
            Tactic::Vertical => format!("{},", texts.join(&format!(",\n{indent}"))),
            Tactic::Mixed => {
                let mut rows: Vec<String> = Vec::new();
                let mut row = String::new();
                for text in &texts {
                    let candidate = if row.is_empty() {
                        format!("{text},")
                    } else {
                        format!("{row} {text},")
                    };
                    if !row.is_empty() && nested.indent + width(&candidate) > MAX_WIDTH {
                        rows.push(std::mem::replace(&mut row, format!("{text},")));
                    } else {
                        row = candidate;
                    }
                }
                rows.push(row);
                rows.join(&format!("\n{indent}"))
            }
        };
        Some((text, tactic))
    }
}

fn can_overflow(item: &Expr, count: usize) -> bool {
    match item.kind() {
        Kind::Closure => true,
        Kind::Array | Kind::Call | Kind::Chain => count == 1,
        Kind::Atom => false,
    }
}

fn flat_list(items: &[Expr]) -> String {
    items.iter().map(Expr::flat).collect::<Vec<_>>().join(", ")
}

/// Whether a rewrite fits `shape`: its first line in the width, every
/// other line in `max_width`, its last line in the shape's extent.
fn fits(text: &str, shape: Shape) -> bool {
    if first_width(text) > shape.width {
        return false;
    }
    if !text.contains('\n') {
        return true;
    }
    text.split('\n')
        .skip(1)
        .all(|line| width(line) <= MAX_WIDTH)
        && last_width(text) <= shape.used() + shape.width
}

/// Whether a chain's last link may continue the root's last line: the
/// line holds nothing but closing delimiters and spaces.
fn last_line_extendable(text: &str) -> bool {
    let line = text.rsplit('\n').next().unwrap_or("");
    line.chars()
        .all(|c| matches!(c, ')' | ']' | '}' | '?' | '>' | ' '))
}

fn width(text: &str) -> usize {
    text.chars().count()
}

fn first_line(text: &str) -> &str {
    text.split('\n').next().unwrap_or("")
}

fn first_width(text: &str) -> usize {
    width(first_line(text))
}

fn last_width(text: &str) -> usize {
    width(text.rsplit('\n').next().unwrap_or(""))
}

fn lines(text: &str) -> usize {
    text.matches('\n').count()
}

fn spaces(count: usize) -> String {
    " ".repeat(count)
}
