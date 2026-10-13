//! Streaming KDL v2 parser: borrowed pull events, source coordinates,
//! and fail-closed limits. Derived from just-kdl and kdlite (MIT OR
//! Apache-2.0); attribution in src/NOTICE.md.
mod chars;
mod coords;
mod error;
mod event;
mod limits;
mod number;
mod parse;
mod scan;

pub use coords::{Locator, locate};
pub use error::{Construct, ErrorKind, Limit, ParseError};
pub use event::{Entry, Event, Number, NumberKind, Radix, Spanned, Value};
pub use limits::Limits;
pub use parse::Parser;
