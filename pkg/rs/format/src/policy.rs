//! What a schema tells the formatter: line limits for the document and
//! per node declaration, the order properties and children are written
//! in, which values are written bare, and which strings are prose or
//! verbatim text. A node is matched to its rule as the binder dispatches
//! it: by its name and its parent's name.

use std::borrow::Cow;
use std::collections::HashMap;

use kdlith_parse::tree::{Block, Item, ItemKind, Node, Part, Spelling};
use kdlith_parse::{Value, is_bare_string};

use crate::Limits;

/// The narrowest a schema may make a line: a smaller width or content
/// width is read as this.
pub const MIN_WIDTH: usize = 20;

/// Formatting rules derived from a schema.
#[derive(Clone, Debug, Default)]
pub struct Policy {
    width: Option<usize>,
    content_width: Option<usize>,
    order: Option<Order>,
    rules: Vec<NodeRule>,
    by_name: HashMap<String, Vec<usize>>,
}

/// The index of a rule in its policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleId(usize);

/// The order a node's properties and children are written in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// As the declaration binds them: the default.
    #[default]
    Declared,
    /// As the document writes them.
    Authored,
}

/// How a string value is laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Text {
    /// Kept exactly: a long value wraps only through whitespace escapes.
    #[default]
    Exact,
    /// Paragraphs whose line breaks carry no meaning: refilled.
    Prose,
    /// Never wrapped over lines it does not hold.
    Verbatim,
}

/// How the value at one argument position or property is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValueRule {
    /// Written bare where a bare string reads back as the same value.
    pub bare: bool,
    pub text: Text,
}

impl ValueRule {
    pub fn new(bare: bool, text: Text) -> Self {
        Self { bare, text }
    }
}

/// The rule for one node declaration.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NodeRule {
    pub name: String,
    /// The node may stand at the root of a document.
    pub root: bool,
    /// The names of the nodes it may stand under.
    pub parents: Vec<String>,
    /// Line limits and order for the node and everything under it, until
    /// a rule further down sets its own.
    pub width: Option<usize>,
    pub content_width: Option<usize>,
    pub order: Option<Order>,
    /// Under the declared order, properties are written after the
    /// arguments, by this rank (the field binding them); keys of one rank
    /// keep their written order, and keys with no rank come last.
    pub property_ranks: HashMap<String, usize>,
    /// Under the declared order, children are written by this rank (the
    /// field or rule that admits them), children of one rank keeping
    /// their written order; a child with no rank stays where it is, and
    /// nothing moves across it. `None` keeps every child where it is.
    pub child_ranks: Option<HashMap<String, usize>>,
    /// The value rule of each argument position; `rest` covers every
    /// position from `args.len()` on.
    pub args: Vec<ValueRule>,
    pub rest: ValueRule,
    pub properties: HashMap<String, ValueRule>,
}

impl NodeRule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    fn arg(&self, position: usize) -> ValueRule {
        self.args.get(position).copied().unwrap_or(self.rest)
    }
}

/// The text kind of every entry the policy marked, by the entry's span.
pub(crate) type Marks = HashMap<(u32, u32), Text>;

impl Policy {
    /// A policy with the document's line limits and order, as far as the
    /// schema sets them.
    pub fn new(width: Option<usize>, content_width: Option<usize>, order: Option<Order>) -> Self {
        Self {
            width,
            content_width,
            order,
            ..Self::default()
        }
    }

    /// Replace the document's limits where `width` or `content_width`
    /// is given, and drop every per-node limit of that kind: a limit
    /// named on the command line holds for the whole document.
    pub fn override_limits(&mut self, width: Option<usize>, content_width: Option<usize>) {
        if width.is_some() {
            self.width = width;
            self.rules.iter_mut().for_each(|rule| rule.width = None);
        }
        if content_width.is_some() {
            self.content_width = content_width;
            self.rules
                .iter_mut()
                .for_each(|rule| rule.content_width = None);
        }
    }

    pub fn add(&mut self, rule: NodeRule) {
        let at = self.rules.len();
        self.by_name.entry(rule.name.clone()).or_default().push(at);
        self.rules.push(rule);
    }

    pub(crate) fn root_limits(&self, base: Limits) -> Limits {
        Limits {
            width: self.width.unwrap_or(base.width).max(MIN_WIDTH),
            content_width: self
                .content_width
                .unwrap_or(base.content_width)
                .max(MIN_WIDTH),
        }
    }

    /// The rule a node named `name` under a node named `parent` (`None`
    /// at the root) binds through.
    pub(crate) fn resolve(&self, parent: Option<&str>, name: &str) -> Option<RuleId> {
        let candidates = self.by_name.get(name)?;
        candidates
            .iter()
            .copied()
            .find(|at| {
                let rule = &self.rules[*at];
                match parent {
                    None => rule.root,
                    Some(parent) => rule.parents.iter().any(|name| name == parent),
                }
            })
            .map(RuleId)
    }

    pub(crate) fn limits(&self, rule: RuleId, inherited: Limits) -> Limits {
        let rule = &self.rules[rule.0];
        Limits {
            width: rule
                .width
                .map_or(inherited.width, |width| width.max(MIN_WIDTH)),
            content_width: rule
                .content_width
                .map_or(inherited.content_width, |width| width.max(MIN_WIDTH)),
        }
    }

    /// Rewrite the document as the rules say, and mark each prose and
    /// verbatim string by its entry's span.
    pub(crate) fn apply(&self, block: &mut Block<'_>) -> Marks {
        let mut marks = Marks::new();
        self.apply_block(block, None, self.order.unwrap_or_default(), &mut marks);
        marks
    }

    fn apply_block(
        &self,
        block: &mut Block<'_>,
        parent: Option<&str>,
        order: Order,
        marks: &mut Marks,
    ) {
        for item in &mut block.items {
            let ItemKind::Node(node) = &mut item.kind else {
                continue;
            };
            let name = node.name.as_str().unwrap_or_default().to_owned();
            let rule = self.resolve(parent, &name).map(|id| &self.rules[id.0]);
            let order = rule.and_then(|rule| rule.order).unwrap_or(order);
            if let Some(rule) = rule {
                rule.apply(node, order, marks);
            }
            for part in &mut node.parts {
                if let Part::Children(children) = part {
                    self.apply_block(&mut children.block, Some(&name), order, marks);
                    if order == Order::Declared
                        && let Some(ranks) = rule.and_then(|rule| rule.child_ranks.as_ref())
                    {
                        order_children(&mut children.block, ranks);
                    }
                }
            }
        }
    }
}

impl NodeRule {
    fn apply(&self, node: &mut Node<'_>, order: Order, marks: &mut Marks) {
        let mut position = 0;
        for part in &mut node.parts {
            let Part::Entry(entry) = part else { continue };
            let rule = match &entry.key {
                None => {
                    let rule = self.arg(position);
                    if !entry.slashdash {
                        position += 1;
                    }
                    rule
                }
                Some(key) => key
                    .as_str()
                    .and_then(|key| self.properties.get(key))
                    .copied()
                    .unwrap_or_default(),
            };
            if entry.ty.is_some() || entry.value.spelling == Spelling::Bare {
                continue;
            }
            let Value::String(text) = &entry.value.value else {
                continue;
            };
            if rule.text == Text::Prose {
                let folded = fold_prose(text);
                if folded != **text {
                    entry.value.value = Value::String(Cow::Owned(folded));
                }
            }
            let Value::String(text) = &entry.value.value else {
                continue;
            };
            if rule.bare && entry.value.spelling.is_quoted() && is_bare_string(text) {
                entry.value.spelling = Spelling::Bare;
            } else if rule.text != Text::Exact {
                marks.insert((entry.span.start, entry.span.end), rule.text);
            }
        }
        if order == Order::Declared {
            order_entries(node, &self.property_ranks);
        }
    }
}

/// `text` as a prose value reads: paragraphs (separated by lines holding
/// only whitespace) joined by one empty line, each paragraph's runs of
/// whitespace folded to one space. The reading folds a prose value the
/// same way, so a refilled value reads as the value it was.
pub fn fold_prose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, out: &mut String| {
        if !paragraph.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(paragraph);
            paragraph.clear();
        }
    };
    for line in text.split('\n') {
        let mut words = line.split_whitespace().peekable();
        if words.peek().is_none() {
            flush(&mut paragraph, &mut out);
            continue;
        }
        for word in words {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(word);
        }
    }
    flush(&mut paragraph, &mut out);
    out
}

/// Arguments first in their order, then properties by rank; each entry
/// keeps the comments written just before it, and comments after the
/// last entry stay last.
fn order_entries(node: &mut Node<'_>, ranks: &HashMap<String, usize>) {
    let split = node
        .parts
        .iter()
        .position(|part| matches!(part, Part::Children(_)))
        .unwrap_or(node.parts.len());
    let tail = node.parts.split_off(split);
    let mut units: Vec<(usize, Vec<Part<'_>>)> = Vec::new();
    let mut pending = Vec::new();
    for part in node.parts.drain(..) {
        match &part {
            Part::Entry(entry) => {
                let rank = match &entry.key {
                    None => 0,
                    Some(key) => key
                        .as_str()
                        .and_then(|key| ranks.get(key))
                        .map_or(usize::MAX, |rank| rank.saturating_add(1)),
                };
                pending.push(part);
                units.push((rank, std::mem::take(&mut pending)));
            }
            _ => pending.push(part),
        }
    }
    units.sort_by_key(|(rank, _)| *rank);
    node.parts = units.into_iter().flat_map(|(_, parts)| parts).collect();
    node.parts.extend(pending);
    node.parts.extend(tail);
}

/// Children by rank within each run between unranked children and empty
/// lines; a node keeps the comments written before it, and a comment
/// written on its line after it.
fn order_children(block: &mut Block<'_>, ranks: &HashMap<String, usize>) {
    let items = std::mem::take(&mut block.items);
    let mut units: Vec<(Option<usize>, Vec<Item<'_>>)> = Vec::new();
    let mut pending: Vec<Item<'_>> = Vec::new();
    for item in items {
        match &item.kind {
            ItemKind::Comment(_) if item.same_line && pending.is_empty() => {
                match units.last_mut() {
                    Some((_, unit)) => unit.push(item),
                    None => pending.push(item),
                }
            }
            ItemKind::Comment(_) => pending.push(item),
            ItemKind::Node(node) => {
                let rank = node.name.as_str().and_then(|name| ranks.get(name)).copied();
                pending.push(item);
                units.push((rank, std::mem::take(&mut pending)));
            }
        }
    }
    // Only a block whose ranked runs are out of order moves at all, so
    // blank lines and comments of an ordered block stay exactly put.
    let mut out = Vec::with_capacity(units.len());
    let mut run: Vec<(usize, Vec<Item<'_>>)> = Vec::new();
    for (rank, unit) in units {
        // An empty line closes a group its author made: nothing moves
        // across it.
        if unit.first().is_some_and(|item| item.blank_before) {
            flush(&mut run, &mut out);
        }
        match rank {
            Some(rank) => run.push((rank, unit)),
            None => {
                flush(&mut run, &mut out);
                out.extend(unit);
            }
        }
    }
    flush(&mut run, &mut out);
    out.extend(pending);
    block.items = out;
}

/// Sort a run of ranked units into `out`, stably.
fn flush<'src>(run: &mut Vec<(usize, Vec<Item<'src>>)>, out: &mut Vec<Item<'src>>) {
    run.sort_by_key(|(rank, _)| *rank);
    out.extend(run.drain(..).flat_map(|(_, unit)| unit));
}
