//! The interface a processor implements: per-node callbacks that bind
//! against the incoming context and request context effects only at the
//! points the execution rule allows.

use std::collections::VecDeque;

use kdlith_diagnostic::ByteSpan;
use kdlith_model::{Item, NodeId, SourceId};

use crate::context::{Context, FollowingEffects, HeaderEffects};

/// What a processor does with a node it has been offered.
#[derive(Debug)]
#[non_exhaustive]
pub enum Disposition<F> {
    /// Bind the node, keeping its state in a frame until it closes.
    Bind(F),
    /// Ignore the node and its whole subtree; no further callbacks run
    /// for it and it requests no effects.
    Skip,
}

/// How a bound node ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Completion {
    /// The node bound successfully; its following effects apply.
    Complete,
    /// The node failed; its following effects are discarded.
    Failed,
}

/// The view a callback gets: the context the node is bound against, its
/// position, and the queue of items the session will yield.
pub struct Cx<'a> {
    pub(crate) context: &'a Context,
    pub(crate) items: &'a mut VecDeque<Item>,
    pub(crate) source: SourceId,
    pub(crate) node: NodeId,
    pub(crate) depth: usize,
    pub(crate) span: ByteSpan,
}

impl Cx<'_> {
    /// The node's incoming context: effects of earlier siblings and
    /// enclosing nodes, never the node's own.
    pub fn context(&self) -> &Context {
        self.context
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    /// The current node, numbered in document order within the source.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Nesting depth of the current node; root nodes are at depth 0.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// The span of the event being handled.
    pub fn span(&self) -> ByteSpan {
        self.span
    }

    /// Queue an item; the session yields queued items in order.
    pub fn emit(&mut self, item: impl Into<Item>) {
        self.items.push_back(item.into());
    }
}

/// A processor over a stream of syntax events. The session calls, per
/// node: [`Processor::open`], [`Processor::entry`] for each argument and
/// property, [`Processor::header`] once the header is complete, then the
/// children's callbacks, then [`Processor::close`].
pub trait Processor {
    /// The node-start payload of the event source.
    type Node;
    /// The entry payload of the event source.
    type Entry;
    /// Per-node binding state, kept on the session's bounded frame stack.
    type Frame;

    /// Offer a node, with the frame of its parent unless it is a root.
    fn open(
        &mut self,
        cx: &mut Cx<'_>,
        parent: Option<&mut Self::Frame>,
        node: Self::Node,
    ) -> Disposition<Self::Frame>;

    /// Bind one argument or property of the open node.
    fn entry(&mut self, cx: &mut Cx<'_>, frame: &mut Self::Frame, entry: Self::Entry);

    /// The header is complete. Children effects are requested here and
    /// nowhere else, so they depend on the header alone; they apply when
    /// the children block opens and are dropped if there is none.
    fn header(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &mut Self::Frame,
        has_children: bool,
        children: &mut HeaderEffects<'_>,
    ) {
        let _ = (cx, frame, has_children, children);
    }

    /// The node ended, after its children and with the children's context
    /// already restored. Following effects apply to later siblings only
    /// when the node completes.
    fn close(
        &mut self,
        cx: &mut Cx<'_>,
        parent: Option<&mut Self::Frame>,
        frame: Self::Frame,
        following: &mut FollowingEffects<'_>,
    ) -> Completion;

    /// The input ended cleanly with every node closed, before the
    /// document's scope completion is yielded: the point for checks over
    /// the document's root nodes. `cx.node()` is one past the last node.
    fn finish(&mut self, cx: &mut Cx<'_>) {
        let _ = cx;
    }
}
