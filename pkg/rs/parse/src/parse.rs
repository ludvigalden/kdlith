//! The document-level state machine: nodes, entries, children blocks,
//! and slashdash suppression.

use kdlith_diagnostic::ByteSpan;

use crate::error::{Construct, ErrorKind, Limit, ParseError, offset};
use crate::event::{Entry, Event, Spanned, Value};
use crate::limits::Limits;
use crate::scan::{Result, Scanner};

/// A streaming KDL v2 pull parser over a borrowed source. Each
/// [`Parser::next_event`] call yields the document's next event; slashdashed
/// nodes, entries, and children blocks are validated, then suppressed. The
/// first error ends the stream.
pub struct Parser<'src> {
    scan: Scanner<'src>,
    state: State,
    blocks: Vec<Block>,
    /// The error that ended the stream, repeated by every later pull.
    failed: Option<ParseError>,
}

#[derive(Clone, Copy)]
enum State {
    Begin,
    Nodes,
    Node { node: Node, spaced: bool },
    Done,
}

/// The open node.
#[derive(Clone, Copy)]
struct Node {
    /// Slashdashed, or inside a slashdashed subtree: nothing it holds is
    /// emitted.
    hidden: bool,
    /// Arguments and properties may still follow.
    entries: bool,
    /// The real children block closed, and with it the node's `End`.
    ended: bool,
}

/// An open children block and the node that owns it.
struct Block {
    open: usize,
    slashdashed: bool,
    owner: Node,
}

impl Block {
    fn hidden(&self) -> bool {
        self.slashdashed || self.owner.hidden
    }
}

/// One parsed step; hidden steps are validated but never emitted.
struct Step<'src> {
    event: Event<'src>,
    start: usize,
    end: usize,
    hidden: bool,
}

impl<'src> Parser<'src> {
    /// A parser with the default [`Limits`].
    pub fn new(source: &'src str) -> Self {
        Self::with_limits(source, Limits::DEFAULT)
    }

    pub fn with_limits(source: &'src str, limits: Limits) -> Self {
        Self {
            scan: Scanner::new(source, limits),
            state: State::Begin,
            blocks: Vec::new(),
            failed: None,
        }
    }

    /// Pull the next visible event. `Ok(None)` is a clean end of input:
    /// every block closed and trailing trivia checked. After an error the
    /// stream is finished and every later call returns that error again.
    pub fn next_event(&mut self) -> std::result::Result<Option<Spanned<Event<'src>>>, ParseError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        loop {
            match self.step() {
                Ok(None) => return Ok(None),
                Ok(Some(step)) if step.hidden => {}
                Ok(Some(step)) => {
                    let span = ByteSpan::new(offset(step.start), offset(step.end));
                    return Ok(Some(Spanned::new(step.event, span)));
                }
                Err(error) => {
                    self.state = State::Done;
                    self.failed = Some(error.clone());
                    return Err(error);
                }
            }
        }
    }

    fn hidden_scope(&self) -> bool {
        self.blocks.last().is_some_and(Block::hidden)
    }

    fn step(&mut self) -> Result<Option<Step<'src>>> {
        loop {
            let step = match self.state {
                State::Begin => {
                    self.begin()?;
                    None
                }
                State::Nodes => match self.nodes()? {
                    Some(step) => Some(step),
                    None if matches!(self.state, State::Done) => return Ok(None),
                    None => None,
                },
                State::Node { node, spaced } => self.node(node, spaced)?,
                State::Done => return Ok(None),
            };
            if let Some(step) = step {
                return Ok(Some(step));
            }
        }
    }

    fn begin(&mut self) -> Result<()> {
        let max = self.scan.limits().max_source_bytes;
        let len = self.scan.source_len();
        if len > max {
            let kind = ErrorKind::LimitExceeded {
                limit: Limit::SourceBytes,
                max,
            };
            return Err(ParseError::new(kind, 0, max));
        }
        self.scan.bom();
        self.state = State::Nodes;
        Ok(())
    }

    /// Between nodes: the next node, a block close, or the end of input.
    fn nodes(&mut self) -> Result<Option<Step<'src>>> {
        self.scan.line_space()?;
        let start = self.scan.at();
        match self.scan.peek() {
            None => {
                if let Some(block) = self.blocks.last() {
                    let kind = ErrorKind::Unterminated(Construct::Children);
                    return Err(ParseError::new(kind, block.open, start));
                }
                self.state = State::Done;
                Ok(None)
            }
            Some('}') => self.close(start),
            Some(_) => {
                let slashdashed = self.scan.slashdash()?;
                let hidden = slashdashed || self.hidden_scope();
                self.scan.set_skip(hidden);
                let ty = self.scan.type_annotation()?;
                self.scan.node_space()?;
                let name = self.scan.string()?;
                let node = Node {
                    hidden,
                    entries: true,
                    ended: false,
                };
                self.state = State::Node {
                    node,
                    spaced: false,
                };
                Ok(Some(Step {
                    event: Event::Node { ty, name },
                    start,
                    end: self.scan.at(),
                    hidden,
                }))
            }
        }
    }

    /// A `}`: the owning node resumes, and a real block ends it.
    fn close(&mut self, start: usize) -> Result<Option<Step<'src>>> {
        let Some(block) = self.blocks.pop() else {
            return Err(self.scan.error_here(ErrorKind::UnmatchedClose));
        };
        self.scan.bump_byte();
        let mut owner = block.owner;
        owner.entries = false;
        let ended = !block.slashdashed;
        owner.ended |= ended;
        self.state = State::Node {
            node: owner,
            spaced: false,
        };
        Ok(ended.then_some(Step {
            event: Event::End,
            start,
            end: self.scan.at(),
            hidden: owner.hidden,
        }))
    }

    /// Inside a node: an entry, a children block, or a terminator.
    fn node(&mut self, mut node: Node, spaced: bool) -> Result<Option<Step<'src>>> {
        let spaced = self.scan.node_space()? || spaced;
        let start = self.scan.at();
        let terminated = match self.scan.peek() {
            None | Some('}') => true,
            Some(';') => {
                self.scan.bump_byte();
                true
            }
            Some(_) if self.scan.at_line_comment() => {
                self.scan.bump_byte();
                self.scan.bump_byte();
                self.scan.line_comment()?;
                true
            }
            Some(_) => self.scan.newline(),
        };
        if terminated {
            self.state = State::Nodes;
            return Ok((!node.ended).then_some(Step {
                event: Event::End,
                start,
                end: self.scan.at(),
                hidden: node.hidden,
            }));
        }
        let slashdashed = self.scan.slashdash()?;
        let start = self.scan.at();
        if self.scan.peek() == Some('{') {
            return self.open(node, slashdashed, start);
        }
        if !node.entries {
            return Err(self.scan.error_here(ErrorKind::EntryAfterChildren));
        }
        if !slashdashed && !spaced {
            return Err(self.scan.error_here(ErrorKind::ExpectedSpace));
        }
        let hidden = node.hidden || slashdashed;
        self.scan.set_skip(hidden);
        let (entry, spaced) = self.entry()?;
        let end = self.scan.at();
        node.entries = true;
        self.state = State::Node { node, spaced };
        Ok(Some(Step {
            event: Event::Entry(entry),
            start,
            end,
            hidden,
        }))
    }

    /// A `{`: push a block, checked against the depth limit first.
    fn open(&mut self, node: Node, slashdashed: bool, start: usize) -> Result<Option<Step<'src>>> {
        if !slashdashed && node.ended {
            return Err(self.scan.error_here(ErrorKind::MultipleChildren));
        }
        let max = self.scan.limits().max_depth;
        if self.blocks.len() >= max {
            let kind = ErrorKind::LimitExceeded {
                limit: Limit::Depth,
                max,
            };
            return Err(self.scan.error_here(kind));
        }
        self.scan.bump_byte();
        let block = Block {
            open: start,
            slashdashed,
            owner: node,
        };
        let hidden = block.hidden();
        self.blocks.push(block);
        self.state = State::Nodes;
        Ok((!slashdashed).then_some(Step {
            event: Event::Children,
            start,
            end: self.scan.at(),
            hidden,
        }))
    }

    /// `node-prop-or-arg`, and whether node space followed an argument.
    fn entry(&mut self) -> Result<(Entry<'src>, bool)> {
        if let Some(ty) = self.scan.type_annotation()? {
            self.scan.node_space()?;
            let value = self.scan.value()?;
            let entry = Entry {
                name: None,
                ty: Some(ty),
                value,
            };
            return Ok((entry, false));
        }
        let value = self.scan.value()?;
        let spaced = self.scan.node_space()?;
        let Value::String(key) = value else {
            let entry = Entry {
                name: None,
                ty: None,
                value,
            };
            return Ok((entry, spaced));
        };
        if self.scan.peek() != Some('=') {
            let entry = Entry {
                name: None,
                ty: None,
                value: Value::String(key),
            };
            return Ok((entry, spaced));
        }
        self.scan.bump_byte();
        self.scan.node_space()?;
        let ty = self.scan.type_annotation()?;
        self.scan.node_space()?;
        let value = self.scan.value()?;
        let entry = Entry {
            name: Some(key),
            ty,
            value,
        };
        Ok((entry, false))
    }
}

/// The pull stream as an iterator: each error is yielded once, then the
/// iterator ends.
impl<'src> Iterator for Parser<'src> {
    type Item = std::result::Result<Spanned<Event<'src>>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed.is_some() {
            return None;
        }
        self.next_event().transpose()
    }
}
