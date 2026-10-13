//! The processing session: pulls syntax events, drives a processor
//! through the execution rule, and yields items one at a time.

use std::collections::VecDeque;
use std::fmt;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::{Item, NodeId, ScopeCompletion, SourceId};

use crate::context::{
    Context, ContextEffect, ContextError, ContextLayout, FollowingEffects, HeaderEffects, Scope,
};
use crate::frame::FrameStack;
use crate::processor::{Completion, Cx, Disposition, Processor};
use crate::source::{EventSource, SourceEvent, Step};

/// Resource limits for one session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionLimits {
    pub(crate) max_depth: usize,
}

impl SessionLimits {
    /// The default profile: 512 levels of bound nodes, the parser's own
    /// default nesting limit.
    pub const DEFAULT: Self = Self { max_depth: 512 };

    /// Cap the number of bound nodes open at once.
    pub const fn with_max_depth(mut self, max: usize) -> Self {
        self.max_depth = max;
        self
    }
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a session stopped. A stopped session returns the same error from
/// every later [`Session::next_item`] call, so a failure never reads as
/// the end of the stream.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SessionError {
    /// The event source failed, with its diagnostic.
    Source(Box<DiagnosticRecord>),
    /// A session resource limit was reached before a frame was allocated.
    Limit(Box<DiagnosticRecord>),
    /// The processor requested an effect the context cannot apply.
    Context(ContextError),
    /// The event source broke its structural contract.
    Protocol(&'static str),
}

impl SessionError {
    /// The diagnostic behind a source or limit failure.
    pub fn diagnostic(&self) -> Option<&DiagnosticRecord> {
        match self {
            SessionError::Source(record) | SessionError::Limit(record) => Some(record),
            SessionError::Context(_) | SessionError::Protocol(_) => None,
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Source(record) | SessionError::Limit(record) => {
                let message = record.message.as_deref().unwrap_or(&record.correction);
                write!(f, "{}: {message}", record.code)
            }
            SessionError::Context(error) => error.fmt(f),
            SessionError::Protocol(reason) => write!(f, "event source contract broken: {reason}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<ContextError> for SessionError {
    fn from(error: ContextError) -> Self {
        SessionError::Context(error)
    }
}

/// A bound node's frame on the stack.
struct Open<F> {
    frame: F,
    node: NodeId,
    /// [`Processor::header`] has run.
    header: bool,
    /// The scope entered for the node's children block.
    children: Option<Scope>,
}

enum State {
    Running,
    Finished,
    Failed(SessionError),
}

/// A processing session over one source. Every callback sees the
/// context the execution rule assigns its node:
///
/// 1. a node binds against its incoming context;
/// 2. its effects are requested only after its header is bound;
/// 3. children effects come from the header alone and apply as the
///    children block opens;
/// 4. following effects apply to later siblings only once the node
///    completes successfully;
/// 5. leaving a node's children, or the document, restores the context
///    it was entered with.
pub struct Session<S, P>
where
    S: EventSource,
    P: Processor<Node = S::Node, Entry = S::Entry>,
{
    source: S,
    processor: P,
    context: Context,
    frames: FrameStack<Open<P::Frame>>,
    /// Depth of a skipped subtree still being drained; no frames are kept
    /// for it.
    skipping: usize,
    items: VecDeque<Item>,
    /// Reused buffer for the effects of the node being handled.
    effects: Vec<ContextEffect>,
    document: Option<Scope>,
    source_id: SourceId,
    next_node: u32,
    /// The span of the latest event, where the document ends.
    last: ByteSpan,
    limits: SessionLimits,
    state: State,
}

impl<S, P> Session<S, P>
where
    S: EventSource,
    P: Processor<Node = S::Node, Entry = S::Entry>,
{
    /// A session with default limits and an empty context over `layout`.
    pub fn new(source_id: SourceId, source: S, processor: P, layout: ContextLayout) -> Self {
        Self::with_context(source_id, source, processor, Context::new(layout))
    }

    /// A session starting from a given context, such as an import's
    /// projection of its caller's context.
    pub fn with_context(source_id: SourceId, source: S, processor: P, context: Context) -> Self {
        let mut context = context;
        let document = Some(context.enter());
        Self {
            source,
            processor,
            context,
            frames: FrameStack::new(SessionLimits::DEFAULT.max_depth),
            skipping: 0,
            items: VecDeque::new(),
            effects: Vec::new(),
            document,
            source_id,
            next_node: 0,
            last: ByteSpan::new(0, 0),
            limits: SessionLimits::DEFAULT,
            state: State::Running,
        }
    }

    /// Replace the limits; only meaningful before the first item.
    pub fn with_limits(mut self, limits: SessionLimits) -> Self {
        self.limits = limits;
        self.frames = FrameStack::new(limits.max_depth);
        self
    }

    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub fn limits(&self) -> SessionLimits {
        self.limits
    }

    /// The current context: after the stream ends, the document scope has
    /// been left and the context is back to the one the session began with.
    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn processor(&self) -> &P {
        &self.processor
    }

    pub fn processor_mut(&mut self) -> &mut P {
        &mut self.processor
    }

    /// Number of bound nodes currently open.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Give back the processor and the context.
    pub fn into_parts(self) -> (P, Context) {
        (self.processor, self.context)
    }

    /// Pull the next item. `Ok(None)` is a clean end: every node closed and
    /// the document's scope completion yielded. Items queued before a
    /// failure are yielded first; then the failure repeats on every call.
    pub fn next_item(&mut self) -> Result<Option<Item>, SessionError> {
        loop {
            if let Some(item) = self.items.pop_front() {
                return Ok(Some(item));
            }
            match &self.state {
                State::Failed(error) => return Err(error.clone()),
                State::Finished => return Ok(None),
                State::Running => {}
            }
            let pulled = self.source.next_event();
            let outcome = match pulled {
                Err(record) => Err(SessionError::Source(record)),
                Ok(None) => self.finish(),
                Ok(Some(event)) => self.step(event),
            };
            if let Err(error) = outcome {
                self.state = State::Failed(error);
            }
        }
    }

    fn step(&mut self, event: SourceEvent<S::Node, S::Entry>) -> Result<(), SessionError> {
        let span = event.span;
        self.last = span;
        if self.skipping > 0 {
            match event.step {
                Step::Node(_) => self.skipping += 1,
                Step::End => self.skipping -= 1,
                Step::Entry(_) | Step::Children => {}
            }
            return Ok(());
        }
        match event.step {
            Step::Node(node) => self.open(node, span),
            Step::Entry(entry) => self.entry(entry, span),
            Step::Children => self.children(span),
            Step::End => self.close(span),
        }
    }

    fn open(&mut self, node: S::Node, span: ByteSpan) -> Result<(), SessionError> {
        let id = NodeId::new(self.source_id, self.next_node);
        self.next_node = self.next_node.saturating_add(1);
        if self.frames.is_full() {
            return Err(depth_limit(self.limits.max_depth, span));
        }
        let depth = self.frames.len();
        let mut cx = Cx {
            context: &self.context,
            items: &mut self.items,
            source: self.source_id,
            node: id,
            depth,
            span,
        };
        let parent = self.frames.last_mut().map(|open| &mut open.frame);
        match self.processor.open(&mut cx, parent, node) {
            Disposition::Bind(frame) => {
                let open = Open {
                    frame,
                    node: id,
                    header: false,
                    children: None,
                };
                if self.frames.push(open).is_err() {
                    return Err(depth_limit(self.limits.max_depth, span));
                }
            }
            Disposition::Skip => self.skipping = 1,
        }
        Ok(())
    }

    fn entry(&mut self, entry: S::Entry, span: ByteSpan) -> Result<(), SessionError> {
        let depth = self.frames.len().saturating_sub(1);
        let open = self
            .frames
            .last_mut()
            .ok_or(SessionError::Protocol("entry outside a node"))?;
        if open.header {
            return Err(SessionError::Protocol("entry after the node header"));
        }
        let mut cx = Cx {
            context: &self.context,
            items: &mut self.items,
            source: self.source_id,
            node: open.node,
            depth,
            span,
        };
        self.processor.entry(&mut cx, &mut open.frame, entry);
        Ok(())
    }

    fn children(&mut self, span: ByteSpan) -> Result<(), SessionError> {
        self.header(true, span)?;
        let scope = self.context.enter();
        let open = self
            .frames
            .last_mut()
            .ok_or(SessionError::Protocol("children outside a node"))?;
        if open.children.is_some() {
            self.context.leave(scope)?;
            return Err(SessionError::Protocol("second children block"));
        }
        open.children = Some(scope);
        for effect in self.effects.drain(..) {
            self.context.apply(effect)?;
        }
        Ok(())
    }

    /// Run the header callback once, collecting children effects into the
    /// reused buffer before any child is read.
    fn header(&mut self, has_children: bool, span: ByteSpan) -> Result<(), SessionError> {
        self.effects.clear();
        let depth = self.frames.len().saturating_sub(1);
        let open = self
            .frames
            .last_mut()
            .ok_or(SessionError::Protocol("header outside a node"))?;
        if open.header {
            return Ok(());
        }
        open.header = true;
        let mut cx = Cx {
            context: &self.context,
            items: &mut self.items,
            source: self.source_id,
            node: open.node,
            depth,
            span,
        };
        let mut children = HeaderEffects::new(&mut self.effects);
        self.processor
            .header(&mut cx, &mut open.frame, has_children, &mut children);
        Ok(())
    }

    fn close(&mut self, span: ByteSpan) -> Result<(), SessionError> {
        self.header(false, span)?;
        self.effects.clear();
        let open = self
            .frames
            .pop()
            .ok_or(SessionError::Protocol("end outside a node"))?;
        if let Some(scope) = open.children {
            self.context.leave(scope)?;
        }
        let depth = self.frames.len();
        let mut cx = Cx {
            context: &self.context,
            items: &mut self.items,
            source: self.source_id,
            node: open.node,
            depth,
            span,
        };
        let parent = self.frames.last_mut().map(|parent| &mut parent.frame);
        let mut following = FollowingEffects::new(&mut self.effects);
        let completion = self
            .processor
            .close(&mut cx, parent, open.frame, &mut following);
        if completion == Completion::Complete {
            for effect in self.effects.drain(..) {
                self.context.apply(effect)?;
            }
        } else {
            self.effects.clear();
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), SessionError> {
        if self.skipping > 0 || !self.frames.is_empty() {
            return Err(SessionError::Protocol("input ended inside a node"));
        }
        let mut cx = Cx {
            context: &self.context,
            items: &mut self.items,
            source: self.source_id,
            node: NodeId::new(self.source_id, self.next_node),
            depth: 0,
            span: self.last,
        };
        self.processor.finish(&mut cx);
        if let Some(scope) = self.document.take() {
            self.context.leave(scope)?;
        }
        self.items
            .push_back(Item::ScopeComplete(ScopeCompletion::new(
                self.source_id,
                None,
            )));
        self.state = State::Finished;
        Ok(())
    }
}

/// The registered limit diagnostic for a session depth breach.
fn depth_limit(max: usize, span: ByteSpan) -> SessionError {
    const CODE: &str = "KDLITH-SRC003";
    let mut record =
        DiagnosticRecord::from_code(CODE).expect("KDLITH-SRC003 is registered and never retired");
    if let Some(definition) = lookup(CODE) {
        let message = render(
            definition.message_template,
            &["binding depth", &max.to_string()],
        );
        record = record.with_message(message);
    }
    SessionError::Limit(Box::new(record.with_span(span, None, None)))
}
