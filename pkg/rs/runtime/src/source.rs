//! The syntax-event contract a session pulls from, independent of any
//! one parser.

use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};

/// One structural step of a document. A `Node` is closed by exactly one
/// `End`; a `Children` between them opens the node's child nodes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Step<N, E> {
    Node(N),
    Entry(E),
    Children,
    End,
}

/// A step with the byte span of the source text it was read from.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SourceEvent<N, E> {
    pub step: Step<N, E>,
    pub span: ByteSpan,
}

impl<N, E> SourceEvent<N, E> {
    pub fn new(step: Step<N, E>, span: ByteSpan) -> Self {
        Self { step, span }
    }
}

/// The result of one pull: an event, a clean end of input, or the
/// source's diagnostic.
pub type Pulled<N, E> = Result<Option<SourceEvent<N, E>>, Box<DiagnosticRecord>>;

/// A pull source of syntax events. `Ok(None)` is a clean end of input;
/// an error is final, and every later call must return it again so a
/// failure never reads as end of input.
pub trait EventSource {
    /// The payload of a node start: its name and type annotation.
    type Node;
    /// The payload of an argument or property.
    type Entry;

    fn next_event(&mut self) -> Pulled<Self::Node, Self::Entry>;
}

/// An [`EventSource`] backed by a closure; see [`from_fn`].
#[derive(Clone, Debug)]
pub struct FnSource<F>(F);

/// Adapt a pull closure, typically one mapping a parser's events onto
/// [`SourceEvent`]s, into an [`EventSource`].
pub fn from_fn<N, E, F>(pull: F) -> FnSource<F>
where
    F: FnMut() -> Pulled<N, E>,
{
    FnSource(pull)
}

impl<N, E, F> EventSource for FnSource<F>
where
    F: FnMut() -> Pulled<N, E>,
{
    type Node = N;
    type Entry = E;

    fn next_event(&mut self) -> Pulled<N, E> {
        (self.0)()
    }
}
